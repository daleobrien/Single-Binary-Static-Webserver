import { Outlet } from 'react-router-dom';
import { useEffect, useState } from 'react';
import Nav from './components/Nav';
import Footer from './components/Footer';
import useDynamicStyles from './hooks/useDynamicStyles';
import './App.css';

export default function App() {
  const [hue, setHue] = useState(220);

  useEffect(() => {
    const id = setInterval(() => {
      setHue((h) => (h + 0.4) % 360);
    }, 50);
    return () => clearInterval(id);
  }, []);

  // CSS string regenerated every time `hue` changes — injected live into <head>
  const dynamicCSS = `
    .main {
      background: linear-gradient(
        135deg,
        hsl(${hue}, 30%, 50%) 0%,
        hsl(${(hue + 60) % 360}, 30%, 55%) 50%,
        hsl(${(hue + 120) % 360}, 25%, 45%) 100%
      );
    }
  `;

  useDynamicStyles(dynamicCSS, 'dynamic-bg');

  return (
    <div className="app">
      <Nav />
      <main className="main">
        <Outlet />
      </main>
      <Footer />
    </div>
  );
}
