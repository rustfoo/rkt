use std::io;
use std::pin::Pin;

use crate::data::data_stream::{DataStream, RawReader, RawStream};
use crate::data::peekable::Peekable;
use crate::data::policy::PolicedStream;
use crate::data::transform::{InPlaceMap, Inspect, Transform, TransformBuf};
use crate::data::{BodyPolicy, ByteUnit};

/// Type representing the body data of a request.
///
/// This type is the only means by which the body of a request can be retrieved.
/// This type is not usually used directly. Instead, data guards (types that
/// implement [`FromData`](crate::data::FromData)) are created indirectly via
/// code generation by specifying the `data = "<var>"` route parameter as
/// follows:
///
/// ```rust
/// # #[macro_use] extern crate rkt;
/// # type DataGuard = String;
/// #[post("/submit", data = "<var>")]
/// fn submit(var: DataGuard) { /* ... */ }
/// # fn main() { }
/// ```
///
/// Above, `DataGuard` can be any type that implements `FromData`. Note that
/// `Data` itself implements `FromData`.
///
/// # Reading Data
///
/// Data may be read from a `Data` object by calling either the
/// [`open()`](Data::open()) or [`peek()`](Data::peek()) methods.
///
/// The `open` method consumes the `Data` object and returns the raw data
/// stream. The `Data` object is consumed for safety reasons: consuming the
/// object ensures that holding a `Data` object means that all of the data is
/// available for reading.
///
/// The `peek` method returns a slice containing at most 512 bytes of buffered
/// body data. This enables partially or fully reading from a `Data` object
/// without consuming the `Data` object.
///
/// # Body Policy
///
/// Every read of body data, whether via `peek` or via the stream returned by
/// `open`, is subject to the [`BodyPolicy`] in effect: the body must arrive
/// within the configured progress bound and must not exceed the hard size
/// limit. A read that violates the policy fails with an [`io::Error`] carrying
/// a [`BodyPolicyError`](crate::data::BodyPolicyError), and every read
/// thereafter fails with the same error. The body clock starts when the body
/// becomes available and is never restarted by forwarding or opening the data.
pub struct Data<'r> {
    stream: Peekable<512, RawReader<'r>>,
    transforms: Vec<Pin<Box<dyn Transform + Send + Sync + 'r>>>,
}

impl<'r> Data<'r> {
    #[inline]
    pub(crate) fn new(stream: Peekable<512, RawReader<'r>>) -> Self {
        Self {
            stream,
            transforms: Vec::new(),
        }
    }

    /// Creates a `Data` for a request body that has just become available.
    /// The body clock starts now, and `policy` is enforced on every read.
    #[inline]
    pub(crate) fn from<S: Into<RawStream<'r>>>(stream: S, policy: BodyPolicy) -> Data<'r> {
        let stream = PolicedStream::new(stream.into(), policy);
        Data::new(Peekable::new(RawReader::new(stream)))
    }

    /// Creates a `Data` for a multipart field of a larger body. The field is
    /// read through that body's stream, which counts and bounds every byte it
    /// yields, so the field itself enforces no policy of its own.
    #[inline]
    pub(crate) fn from_field(field: multer::Field<'r>) -> Data<'r> {
        let stream = PolicedStream::inherited(field.into());
        Data::new(Peekable::new(RawReader::new(stream)))
    }

    /// This creates a `data` object from a local data source `data`.
    ///
    /// The data is already in hand, so the progress bound of `policy` cannot
    /// be violated, but its size limit is applied as it would be to a received
    /// body: if `data` exceeds the limit, reading past it fails.
    #[inline]
    pub(crate) fn local(data: Vec<u8>, policy: BodyPolicy) -> Data<'r> {
        let mut stream = PolicedStream::new(RawStream::Empty, policy);
        // A failure is recorded in the stream and surfaces from its first read.
        let _ = stream.count(data.len());
        Data::new(Peekable::with_buffer(data, true, RawReader::new(stream)))
    }

    /// Returns the raw data stream, limited to `limit` bytes.
    ///
    /// The stream contains all of the data in the body of the request,
    /// including that in the `peek` buffer. The method consumes the `Data`
    /// instance. This ensures that a `Data` type _always_ represents _all_ of
    /// the data in a request.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rkt::data::{Data, ToByteUnit};
    ///
    /// # const SIZE_LIMIT: u64 = 2 << 20; // 2MiB
    /// fn handler(data: Data<'_>) {
    ///     let stream = data.open(2.mebibytes());
    /// }
    /// ```
    #[inline(always)]
    pub fn open(self, limit: ByteUnit) -> DataStream<'r> {
        DataStream::new(self.transforms, self.stream, limit.into())
    }

    /// Fills the peek buffer with body data until it contains at least `num`
    /// bytes (capped to 512), or the complete body data, whichever is less, and
    /// returns it. If the buffer already contains either at least `num` bytes
    /// or all of the body data, no I/O is performed and the buffer is simply
    /// returned. If `num` is greater than `512`, it is artificially capped to
    /// `512`.
    ///
    /// No guarantees are made about the actual size of the returned buffer
    /// except that it will not exceed the length of the body data. It may be:
    ///
    ///   * Less than `num` if `num > 512` or the complete body data is `< 512`
    ///     or an error occurred while reading the body.
    ///   * Equal to `num` if `num` is `<= 512` and exactly `num` bytes of the
    ///     body data were successfully read.
    ///   * Greater than `num` if `> num` bytes of the body data have
    ///     successfully been read, either by this request, a previous request,
    ///     or opportunistically.
    ///
    /// [`Data::peek_complete()`] can be used to determine if this buffer
    /// contains the complete body data.
    ///
    /// # Examples
    ///
    /// In a data guard:
    ///
    /// ```rust
    /// use rkt::request::{self, Request, FromRequest};
    /// use rkt::data::{Data, FromData, Outcome};
    /// use rkt::http::Status;
    /// # struct MyType;
    /// # type MyError = String;
    ///
    /// #[rkt::async_trait]
    /// impl<'r> FromData<'r> for MyType {
    ///     type Error = MyError;
    ///
    ///     async fn from_data(r: &'r Request<'_>, mut data: Data<'r>) -> Outcome<'r, Self> {
    ///         if data.peek(2).await != b"hi" {
    ///             return Outcome::Forward((data, Status::BadRequest))
    ///         }
    ///
    ///         /* .. */
    ///         # unimplemented!()
    ///     }
    /// }
    /// ```
    ///
    /// In a fairing:
    ///
    /// ```rust
    /// use rkt::{Rocket, Request, Data, Response};
    /// use rkt::fairing::{Fairing, Info, Kind};
    /// # struct MyType;
    ///
    /// #[rkt::async_trait]
    /// impl Fairing for MyType {
    ///     fn info(&self) -> Info {
    ///         Info {
    ///             name: "Data Peeker",
    ///             kind: Kind::Request
    ///         }
    ///     }
    ///
    ///     async fn on_request(&self, req: &mut Request<'_>, data: &mut Data<'_>) {
    ///         if data.peek(2).await == b"hi" {
    ///             /* do something; body data starts with `"hi"` */
    ///         }
    ///
    ///         /* .. */
    ///         # unimplemented!()
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub async fn peek(&mut self, num: usize) -> &[u8] {
        self.stream.peek(num).await
    }

    /// Returns true if the `peek` buffer contains all of the data in the body
    /// of the request. Returns `false` if it does not or it is not known.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rkt::data::Data;
    ///
    /// async fn handler(mut data: Data<'_>) {
    ///     if data.peek_complete() {
    ///         println!("All of the data: {:?}", data.peek(512).await);
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub fn peek_complete(&self) -> bool {
        self.stream.complete
    }

    /// Chains the [`Transform`] `transform` to `self`.
    ///
    /// Note that transforms do nothing until the data is
    /// [`open()`ed](Data::open()) and read.
    #[inline(always)]
    pub fn chain_transform<T>(&mut self, transform: T) -> &mut Self
    where
        T: Transform + Send + Sync + 'static,
    {
        self.transforms.push(Box::pin(transform));
        self
    }

    /// Chain a [`Transform`] that can inspect the data as it streams.
    pub fn chain_inspect<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&[u8]) + Send + Sync + 'static,
    {
        self.chain_transform(Inspect(Box::new(f)))
    }

    /// Chain a [`Transform`] that can in-place map the data as it streams.
    /// Unlike [`Data::chain_try_inplace_map()`], this version assumes the
    /// mapper is infallible.
    pub fn chain_inplace_map<F>(&mut self, mut f: F) -> &mut Self
    where
        F: FnMut(&mut TransformBuf<'_, '_>) + Send + Sync + 'static,
    {
        self.chain_transform(InPlaceMap(Box::new(move |buf| {
            f(buf);
            Ok(())
        })))
    }

    /// Chain a [`Transform`] that can in-place map the data as it streams.
    /// Unlike [`Data::chain_inplace_map()`], this version allows the mapper to
    /// be infallible.
    pub fn chain_try_inplace_map<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(&mut TransformBuf<'_, '_>) -> io::Result<()> + Send + Sync + 'static,
    {
        self.chain_transform(InPlaceMap(Box::new(f)))
    }
}
