import React from 'react';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Layout from '@theme/Layout';
import CodeBlock from '@theme/CodeBlock';
import styles from './index.module.css';

const HERO_EXAMPLE = `#[macro_use] extern crate rkt;

#[get("/hello/<name>")]
fn hello(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[launch]
fn rocket() -> _ {
    rkt::build().mount("/", routes![hello])
}`;

const TYPED_EXAMPLE = `#[macro_use] extern crate rkt;

#[get("/<name>/<age>")]
fn hello(name: &str, age: u8) -> String {
    format!("Hello, {} year old named {}!", age, name)
}

#[launch]
fn rocket() -> _ {
    rkt::build().mount("/hello", routes![hello])
}`;

const FEATURES = [
  {
    title: 'Type-safe routing',
    description:
      'Route parameters, query strings, and request bodies are parsed into Rust types. If parsing fails, rkt returns 404 — no error handling required in your handler.',
  },
  {
    title: 'Async',
    description:
      'Built on Tokio and Hyper. HTTP/1.1 and HTTP/2 are built in. HTTP/3 is available as a preview feature via s2n-quic.',
  },
  {
    title: 'TLS and mTLS',
    description:
      'TLS and mutual TLS via Rustls. Secret cookies are signed and encrypted. Enable with feature flags.',
  },
  {
    title: 'WebSockets',
    description:
      'WebSocket support via rkt_ws, using the same handler style as HTTP routes.',
  },
  {
    title: 'Fairings',
    description:
      'Middleware that can inspect or modify requests, responses, and lifecycle events. Implemented as a trait rather than a function chain.',
  },
  {
    title: 'Request guards',
    description:
      'Implement FromRequest to parse, validate, or reject requests before they reach your handler. Composable and zero-boilerplate at the call site.',
  },
];

const CRATES = [
  {
    name: 'rkt',
    description: 'Core framework: routing, request guards, responders, fairings, TLS.',
    link: 'https://docs.rs/rkt',
    linkLabel: 'docs.rs/rkt',
  },
  {
    name: 'rkt_ws',
    description: 'WebSocket support.',
    link: 'https://docs.rs/rkt_ws',
    linkLabel: 'docs.rs/rkt_ws',
  },
  {
    name: 'rkt_dyn_templates',
    description: 'Template rendering via Tera, Handlebars, or MiniJinja.',
    link: 'https://docs.rs/rkt_dyn_templates',
    linkLabel: 'docs.rs/rkt_dyn_templates',
  },
];

function FeatureCard({ title, description }) {
  return (
    <div className={styles.featureCard}>
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

function CrateCard({ name, description, link, linkLabel }) {
  return (
    <div className={styles.crateCard}>
      <code className={styles.crateName}>{name}</code>
      <p className={styles.crateDesc}>{description}</p>
      <a href={link} className={styles.crateLink} target="_blank" rel="noopener noreferrer">
        {linkLabel} &rarr;
      </a>
    </div>
  );
}

function Hero() {
  const { siteConfig } = useDocusaurusContext();
  const mascotUrl = useBaseUrl('/img/rkt_mascot.svg');
  return (
    <header className={styles.hero}>
      <div className={styles.heroGlow} aria-hidden="true" />
      <div className={styles.heroInner}>
        <div className={styles.heroText}>
          <div className={styles.heroLockup}>
            <img className={styles.heroMascot} src={mascotUrl} alt="rkt mascot" />
            <h1 className={styles.heroTitle}>{siteConfig.title}</h1>
          </div>
          <p className={styles.heroTagline}>{siteConfig.tagline}</p>
          <p className={styles.heroSubtitle}>
            An async web framework for Rust. Routes are type-checked at compile
            time. Request data is parsed into your types automatically. A
            continuation of the Rocket project.
          </p>
          <div className={styles.heroButtons}>
            <Link className="button button--primary button--lg" to="/guide/introduction">
              Get started
            </Link>
            <Link
              className={`button button--outline button--lg ${styles.heroOutlineBtn}`}
              to="https://github.com/rustfoo/rkt"
            >
              GitHub
            </Link>
          </div>
        </div>
        <div className={styles.heroCode}>
          <CodeBlock language="rust">{HERO_EXAMPLE}</CodeBlock>
        </div>
      </div>
    </header>
  );
}

function Features() {
  return (
    <section className={styles.section}>
      <div className={styles.container}>
        <h2 className={styles.sectionTitle}>Features</h2>
        <div className={styles.featureGrid}>
          {FEATURES.map((f) => (
            <FeatureCard key={f.title} {...f} />
          ))}
        </div>
      </div>
    </section>
  );
}

function QuickStart() {
  return (
    <section className={`${styles.section} ${styles.sectionAlt}`}>
      <div className={styles.container}>
        <div className={styles.quickStartInner}>
          <div className={styles.quickStartText}>
            <h2>Getting started</h2>
            <p>
              Add rkt to <code>Cargo.toml</code>. Route parameters are parsed
              into Rust types — if <code>&lt;age&gt;</code> isn't a valid{' '}
              <code>u8</code>, rkt returns 404 without your handler being called.
            </p>
            <p>
              The <Link to="/guide/introduction">guide</Link> covers routing,
              request guards, state, fairings, and deployment.
            </p>
            <Link className="button button--primary" to="/guide/quickstart">
              Quickstart &rarr;
            </Link>
          </div>
          <div className={styles.quickStartCode}>
            <CodeBlock language="toml" title="Cargo.toml">{`[dependencies]\nrkt = "1.0.0"`}</CodeBlock>
            <CodeBlock language="rust" title="src/main.rs">{TYPED_EXAMPLE}</CodeBlock>
          </div>
        </div>
      </div>
    </section>
  );
}

function Ecosystem() {
  return (
    <section className={styles.section}>
      <div className={styles.container}>
        <h2 className={styles.sectionTitle}>Crates</h2>
        <div className={styles.crateGrid}>
          {CRATES.map((c) => (
            <CrateCard key={c.name} {...c} />
          ))}
        </div>
      </div>
    </section>
  );
}

function CtaBanner() {
  return (
    <section className={styles.ctaBanner}>
      <div className={styles.container}>
        <h2 className={styles.ctaTitle}>Start building</h2>
        <div className={styles.ctaButtons}>
          <Link className="button button--primary button--lg" to="/guide/introduction">
            Read the guide
          </Link>
          <Link
            className={`button button--outline button--lg ${styles.ctaOutlineBtn}`}
            to="https://github.com/rustfoo/rkt"
          >
            GitHub
          </Link>
        </div>
      </div>
    </section>
  );
}

export default function Home() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout description={siteConfig.tagline}>
      <Hero />
      <main>
        <Features />
        <QuickStart />
        <Ecosystem />
        <CtaBanner />
      </main>
    </Layout>
  );
}
