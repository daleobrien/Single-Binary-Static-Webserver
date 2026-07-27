import './Footer.css';

export default function Footer() {
  return (
    <footer className="footer">
      <p>
        Built with React &amp; Vite · Served over TLS ·{' '}
        {new Date().getFullYear()}
      </p>
    </footer>
  );
}
