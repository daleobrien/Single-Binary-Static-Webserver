import { Outlet } from 'react-router-dom';
import Nav from './components/Nav';
import Footer from './components/Footer';
import './App.css';

export default function App() {
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
