import { useState, useEffect } from 'react';
import { useLocalStorage } from '../hooks/useLocalStorage';
import Card from '../components/Card';
import './Dashboard.css';

/* ── helpers ──────────────────────────────────────────────── */

function formatTime(date) {
  return date.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

function formatDate(date) {
  return date.toLocaleDateString(undefined, {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

const TECH_FACTS = [
  'The first computer bug was a real moth found in a relay of the Harvard Mark II in 1947.',
  'The term "byte" was coined by Werner Buchholz in 1956 while working on the IBM Stretch.',
  'JavaScript was created in just 10 days by Brendan Eich in 1995.',
  'The first 1 GB hard drive (IBM 3380, 1980) weighed over 250 kg and cost $81,000.',
  'The QWERTY keyboard layout was designed in the 1870s to slow typists down.',
  'Over 90% of the world\'s currency exists only in digital form.',
  'The first website (info.cern.ch) went live on August 6, 1991.',
  'Linux runs on over 96% of the world\'s top 500 supercomputers.',
  'The name "Bluetooth" comes from a 10th-century Viking king, Harald Bluetooth.',
  'Python was named after Monty Python, not the snake.',
];

/* ── Clock ────────────────────────────────────────────────── */

function Clock() {
  const [now, setNow] = useState(new Date());

  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <Card title="Live Clock" className="widget widget-clock">
      <div className="clock-time">{formatTime(now)}</div>
      <div className="clock-date">{formatDate(now)}</div>
    </Card>
  );
}

/* ── Counter ──────────────────────────────────────────────── */

function Counter() {
  const [count, setCount] = useLocalStorage('dashboard-counter', 0);

  return (
    <Card title="Counter" className="widget widget-counter">
      <div className="counter-value">{count}</div>
      <div className="counter-actions">
        <button className="btn-sm btn-sm-danger" onClick={() => setCount(count - 1)} disabled={count <= -99}>
          −1
        </button>
        <button className="btn-sm btn-sm-neutral" onClick={() => setCount(0)}>Reset</button>
        <button className="btn-sm btn-sm-success" onClick={() => setCount(count + 1)} disabled={count >= 99}>
          +1
        </button>
      </div>
    </Card>
  );
}

/* ── Todo list ────────────────────────────────────────────── */

function TodoList() {
  const [todos, setTodos] = useLocalStorage('dashboard-todos', []);
  const [input, setInput] = useState('');

  const add = (e) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed) return;
    setTodos([...todos, { id: Date.now(), text: trimmed, done: false }]);
    setInput('');
  };

  const toggle = (id) =>
    setTodos(todos.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));

  const remove = (id) => setTodos(todos.filter((t) => t.id !== id));

  const clearDone = () => setTodos(todos.filter((t) => !t.done));

  const doneCount = todos.filter((t) => t.done).length;

  return (
    <Card
      title={
        <span>
          Todo List{' '}
          {todos.length > 0 && (
            <span className="todo-stats">
              {doneCount}/{todos.length}
            </span>
          )}
        </span>
      }
      className="widget widget-todos"
    >
      <form onSubmit={add} className="todo-form">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Add a task..."
          maxLength={120}
          className="todo-input"
        />
        <button type="submit" className="btn-sm btn-sm-primary" disabled={!input.trim()}>
          Add
        </button>
      </form>

      {todos.length === 0 && (
        <p className="todo-empty">No tasks yet. Add one above!</p>
      )}

      <ul className="todo-list">
        {todos.map((t) => (
          <li key={t.id} className={t.done ? 'todo-item done' : 'todo-item'}>
            <label className="todo-label">
              <input type="checkbox" checked={t.done} onChange={() => toggle(t.id)} />
              <span>{t.text}</span>
            </label>
            <button className="todo-remove" onClick={() => remove(t.id)} title="Remove">
              ✕
            </button>
          </li>
        ))}
      </ul>

      {doneCount > 0 && (
        <button className="btn-sm btn-sm-neutral todo-clear" onClick={clearDone}>
          Clear completed
        </button>
      )}
    </Card>
  );
}

/* ── Tech Facts ───────────────────────────────────────────── */

function TechFacts() {
  const [index, setIndex] = useState(() => Math.floor(Math.random() * TECH_FACTS.length));

  const shuffle = () => {
    let next;
    do {
      next = Math.floor(Math.random() * TECH_FACTS.length);
    } while (next === index && TECH_FACTS.length > 1);
    setIndex(next);
  };

  return (
    <Card title="Random Tech Fact" className="widget widget-facts">
      <blockquote className="fact-text">&ldquo;{TECH_FACTS[index]}&rdquo;</blockquote>
      <button className="btn-sm btn-sm-primary fact-next" onClick={shuffle}>
        Another fact
      </button>
    </Card>
  );
}

/* ── Dashboard ────────────────────────────────────────────── */

export default function Dashboard() {
  return (
    <div className="dashboard">
      <h1>Dashboard</h1>
      <p className="dash-desc">Interactive widgets — state persists in your browser.</p>
      <div className="dash-grid">
        <Clock />
        <Counter />
        <TodoList />
        <TechFacts />
      </div>
    </div>
  );
}
