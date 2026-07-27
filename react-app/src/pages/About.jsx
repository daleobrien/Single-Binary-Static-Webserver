import './About.css';

export default function About() {
  return (
    <div className="about">
      <h1>About</h1>

      <div className="about-grid">
        <section className="about-section">
          <h2>The Server</h2>
          <p>
            The backend is a lightweight HTTP/TLS server written in Rust using{' '}
            <code>hyper</code> and <code>rustls</code>. It serves static files from
            the <code>public/</code> directory with support for range requests,
            conditional GET, and automatic MIME-type detection. It can also be run
            inside a minimal Docker container.
          </p>
        </section>

        <section className="about-section">
          <h2>The Frontend</h2>
          <p>
            This React application is bundled with <strong>Vite</strong> and uses{' '}
            <code>react-router-dom</code> for client-side routing with hash-based
            URLs — no server-side routing configuration needed. The theme system
            uses CSS custom properties with a React context provider, persisting
            the user&apos;s choice in <code>localStorage</code>.
          </p>
        </section>

        <section className="about-section">
          <h2>Project Structure</h2>
          <div className="code-block">
            <pre><code>{`small-static-webserver-with-tls/
├── certs/            # TLS certificates
├── public/           # built static assets
├── react-app/        # React source (Vite)
│   ├── src/
│   │   ├── components/  # shared UI
│   │   ├── pages/       # route pages
│   │   ├── context/     # React context
│   │   └── hooks/       # custom hooks
│   └── vite.config.js
├── rust-server/      # Rust HTTP/TLS server
└── go.sh             # build & run script`}</code></pre>
          </div>
        </section>
      </div>
    </div>
  );
}
