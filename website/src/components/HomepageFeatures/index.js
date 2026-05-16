import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

const FeatureList = [
  {
    icon: '🛡️',
    title: 'Type-Safe by Default',
    description: (
      <>
        Rocket's macro-powered routing turns URL parameters, query strings, and
        form data into validated Rust types at compile time — whole classes of
        bugs simply cannot exist.
      </>
    ),
  },
  {
    icon: '🔒',
    title: 'Request Guards',
    description: (
      <>
        Express authentication, rate-limiting, and input validation as reusable
        Rust types. If a guard fails, the handler never runs — safety is
        structural, not bolted on.
      </>
    ),
  },
  {
    icon: '⚡',
    title: 'Async & Fast',
    description: (
      <>
        Built on Tokio, rkt handles thousands of concurrent connections with
        fully async handlers. Zero-cost abstractions mean you pay only for what
        you use.
      </>
    ),
  },
  {
    icon: '🧩',
    title: 'Fairings',
    description: (
      <>
        Rocket's fairing system gives you structured lifecycle hooks — attach
        CORS headers, log requests, or instrument metrics without touching your
        route logic.
      </>
    ),
  },
  {
    icon: '📝',
    title: 'First-Class Forms & JSON',
    description: (
      <>
        Derive <code>FromForm</code> on any struct for automatic validation with
        rich error messages. Add <code>serde</code> and get JSON in and out with
        a single type annotation.
      </>
    ),
  },
  {
    icon: '🧪',
    title: 'Built-In Testing',
    description: (
      <>
        Rocket ships a full in-process HTTP client so you can integration-test
        your entire application — routing, guards, and all — without starting a
        server.
      </>
    ),
  },
];

function Feature({icon, title, description}) {
  return (
    <div className={clsx('col col--4', styles.featureCard)}>
      <div className={styles.featureIcon}>{icon}</div>
      <div>
        <Heading as="h3" className={styles.featureTitle}>{title}</Heading>
        <p className={styles.featureDesc}>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures() {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className={clsx('row', styles.featureRow)}>
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
