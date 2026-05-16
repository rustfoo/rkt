import React from 'react';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import CodeBlock from '@theme/CodeBlock';
import styles from './index.module.css';

const CODE_EXAMPLE = `\
#[macro_use] extern crate rocket;

#[get("/hello/<name>")]
fn hello(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![hello])
}`;

function Hero() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={styles.hero}>
      <div className={styles.heroInner}>
        <div className={styles.heroText}>
          <h1 className={styles.heroTitle}>{siteConfig.title}</h1>
          <p className={styles.heroTagline}>{siteConfig.tagline}</p>
          <p className={styles.heroSubtitle}>
            Write fast, type-safe web applications in Rust with minimal boilerplate.
            rkt keeps what made Rocket great — ergonomics, safety, and speed — and
            keeps moving forward.
          </p>
          <div className={styles.heroButtons}>
            <Link className="button button--primary button--lg" to="/guide/introduction">
              Get Started
            </Link>
            <Link
              className="button button--outline button--lg"
              to="https://github.com/rustfoo/rkt"
            >
              GitHub
            </Link>
          </div>
        </div>
        <div className={styles.heroCode}>
          <CodeBlock language="rust">{CODE_EXAMPLE}</CodeBlock>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout description={siteConfig.tagline}>
      <Hero />
    </Layout>
  );
}
