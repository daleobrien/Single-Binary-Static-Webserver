import './NotFound.css';
import { Link } from 'react-router-dom';

export default function NotFound() {
  return (
    <div className="not-found">
      <span className="nf-code">404</span>
      <h1>Page not found</h1>
      <p>The page you&apos;re looking for doesn&apos;t exist or has been moved.</p>
      <Link to="/" className="btn btn-primary">
        Back to Home
      </Link>
    </div>
  );
}
