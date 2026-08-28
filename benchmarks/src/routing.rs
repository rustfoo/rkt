use criterion::{black_box, criterion_group, BenchmarkId, Criterion, Throughput};

use rkt::http::{Accept, ContentType, Method, RawStr, Status};
use rkt::local::blocking::{Client, LocalRequest};
use rkt::{config, route, Config, Data, Request, Route};

fn dummy_handler<'r>(req: &'r Request, _: Data<'r>) -> route::BoxFuture<'r> {
    route::Outcome::from(req, ()).pin()
}

fn parse_routes_table(table: &str) -> Vec<Route> {
    let mut routes = vec![];
    for line in table.split("\n").filter(|s| !s.is_empty()) {
        let mut components = line.split(" ");
        let method: Method = components.next().expect("c").parse().expect("method");
        let uri: &str = components.next().unwrap();

        let (mut rank, mut name, mut format) = (None, None, None);
        for component in components {
            match component {
                c if c.starts_with('[') => rank = c.trim_matches(&['[', ']'][..]).parse().ok(),
                c if c.starts_with('(') => name = Some(c.trim_matches(&['(', ')'][..])),
                c => format = c.parse().ok(),
            }
        }

        let mut route = Route::new(method, uri, dummy_handler);
        if let Some(rank) = rank {
            route.rank = rank;
        }

        route.format = format;
        route.name = name.map(|s| s.to_string().into());
        routes.push(route);
    }

    routes
}

fn generate_matching_requests<'c>(client: &'c Client, routes: &[Route]) -> Vec<LocalRequest<'c>> {
    fn staticify_segment(segment: &RawStr) -> &str {
        segment.as_str().trim_matches(&['<', '>', '.', '_'][..])
    }

    fn request_for_route<'c>(client: &'c Client, route: &Route) -> LocalRequest<'c> {
        let path = route
            .uri
            .path()
            .raw_segments()
            .map(staticify_segment)
            .collect::<Vec<_>>()
            .join("/");

        let query = route
            .uri
            .query()
            .map(|q| q.raw_segments())
            .into_iter()
            .flatten()
            .map(staticify_segment)
            .collect::<Vec<_>>()
            .join("&");

        let uri = format!("/{}?{}", path, query);
        let mut req = client.req(route.method.unwrap(), uri);
        if let Some(ref format) = route.format {
            if let Some(true) = route.method.and_then(|m| m.allows_request_body()) {
                req.add_header(ContentType::from(format.clone()));
            } else {
                req.add_header(Accept::from(format.clone()));
            }
        }

        req
    }

    routes
        .iter()
        .map(|route| request_for_route(client, route))
        .collect()
}

fn client(routes: Vec<Route>) -> Client {
    let config = Config {
        profile: Config::RELEASE_PROFILE,
        log_level: None,
        cli_colors: config::CliColors::Never,
        shutdown: config::ShutdownConfig {
            ctrlc: false,
            #[cfg(unix)]
            signals: std::collections::hash_set::HashSet::new(),
            ..Default::default()
        },
        ..Default::default()
    };

    match Client::untracked(rkt::custom(config).mount("/", routes)) {
        Ok(client) => client,
        Err(e) => {
            drop(e);
            panic!("bad launch")
        }
    }
}

pub fn bench_rust_lang_routes(c: &mut Criterion) {
    let table = include_str!("../static/rust-lang.routes");
    let routes = parse_routes_table(table);
    let client = client(routes.clone());
    let requests = generate_matching_requests(&client, &routes);

    for request in requests.clone() {
        assert_eq!(request.dispatch().status(), Status::Ok);
    }

    c.bench_function("rust-lang.routes", |b| {
        b.iter(|| {
            for request in requests.clone() {
                let response = request.dispatch();
                black_box(response.status());
                black_box(response);
            }
        })
    });
}

pub fn bench_bitwarden_routes(c: &mut Criterion) {
    let table = include_str!("../static/bitwarden_rs.routes");
    let routes = parse_routes_table(table);
    let client = client(routes.clone());
    let requests = generate_matching_requests(&client, &routes);

    for request in requests.clone() {
        assert_eq!(request.dispatch().status(), Status::Ok);
    }

    c.bench_function("bitwarden_rs.routes", |b| {
        b.iter(|| {
            for request in requests.clone() {
                let response = request.dispatch();
                black_box(response.status());
                black_box(response);
            }
        })
    });
}

fn generated_routes(count: usize, dynamic: bool) -> Vec<Route> {
    (0..count)
        .map(|index| {
            let uri = if dynamic {
                format!("/dynamic/{index:05}/<value>")
            } else {
                format!("/static/{index:05}")
            };

            Route::new(Method::Get, &uri, dummy_handler)
        })
        .collect()
}

pub fn bench_linear_route_matching(c: &mut Criterion) {
    let client = client(vec![]);
    let mut group = c.benchmark_group("linear-route-match");
    group.sample_size(30);

    for count in [10usize, 100, 1_000, 10_000] {
        for dynamic in [false, true] {
            let kind = if dynamic { "dynamic" } else { "static" };
            let routes = generated_routes(count, dynamic);
            let cases = [
                ("first", Some(0)),
                ("middle", Some(count / 2)),
                ("last", Some(count - 1)),
                ("missing", None),
            ];

            for (case, expected) in cases {
                let uri = match (dynamic, expected) {
                    (true, Some(index)) => format!("/dynamic/{index:05}/value"),
                    (false, Some(index)) => format!("/static/{index:05}"),
                    (_, None) => "/not-found".into(),
                };
                let request = client.get(uri);
                let lookup = || {
                    routes
                        .iter()
                        .position(|route| route.matches(request.inner()))
                };
                assert_eq!(lookup(), expected);

                let scanned = expected.map_or(count, |index| index + 1);
                group.throughput(Throughput::Elements(scanned as u64));
                group.bench_with_input(
                    BenchmarkId::new(format!("{kind}-{case}"), count),
                    &request,
                    |b, request| {
                        b.iter(|| {
                            black_box(
                                routes
                                    .iter()
                                    .position(|route| route.matches(black_box(request.inner()))),
                            )
                        })
                    },
                );
            }
        }
    }

    group.finish();
}

criterion_group!(
    routing,
    bench_rust_lang_routes,
    bench_bitwarden_routes,
    bench_linear_route_matching,
);
