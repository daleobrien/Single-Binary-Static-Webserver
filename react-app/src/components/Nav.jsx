import { NavLink } from 'react-router-dom';
import ThemeToggle from './ThemeToggle';
import './Nav.css';

const links = [
  { to: '/', label: 'Home' },
  { to: '/about', label: 'About' },
  { to: '/dashboard', label: 'Dashboard' },
];

export default function Nav() {
  return (
    <header className="nav-header">
      <div className="nav-inner">
        <NavLink to="/" className="nav-brand">
          <svg width="24" height="24" viewBox="0 0 32 32" className="nav-logo">
            <defs>
              <linearGradient id="nav-grad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#667eea" />
                <stop offset="100%" stopColor="#764ba2" />
              </linearGradient>
            </defs>
            <rect width="32" height="32" rx="6" fill="url(#nav-grad)" />
            <text x="16" y="23" textAnchor="middle" fontFamily="monospace" fontSize="20" fontWeight="bold" fill="white">R</text>
          </svg>
          <span>StaticServer</span>
        </NavLink>
        <nav className="nav-links">
          {links.map(({ to, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <ThemeToggle />
      </div>
    </header>
  );
}
