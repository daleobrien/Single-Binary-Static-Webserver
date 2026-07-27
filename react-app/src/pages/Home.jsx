import { Link } from 'react-router-dom';
import './Home.css';

const features = [
  {
    icon: '🔒',
    title: 'TLS Encrypted',
    desc: 'All traffic is secured with TLS encryption out of the box.',
  },
  {
    icon: '⚡',
    title: 'Blazing Fast',
    desc: 'Serving pre-built static assets with zero server-side overhead.',
  },
  {
    icon: '🎨',
    title: 'Dark Mode',
    desc: 'Respects your system preference and remembers your choice.',
  },
];

export default function Home() {
  return (
    <div className="home">
      <section className="hero">
        <h1>Static Web Server</h1>
        <p className="hero-sub">
          A minimal, secure static file server written in Rust, paired with a
          modern React frontend. Edit the source, hit build, and you&apos;re live.
        </p>
        <div className="hero-actions">
          <Link to="/dashboard" className="btn btn-primary">
            Try the Dashboard
          </Link>
          <Link to="/about" className="btn btn-outline">
            Learn More
          </Link>
        </div>
      </section>

      <section className="features">
        {features.map(({ icon, title, desc }) => (
          <div key={title} className="feature-card">
            <span className="feature-icon">{icon}</span>
            <h3>{title}</h3>
            <p>{desc}</p>
          </div>
        ))}
      </section>

      <section className="quick-start">
        <h2>Quick Start</h2>
        <div className="code-block">
          <pre><code>{`cd react-app
npm install
npm run build       # outputs to ../public/

# serve with the Rust server:
cd .. && ./go.sh`}</code></pre>
        </div>
      </section>
    </div>
  );
}
