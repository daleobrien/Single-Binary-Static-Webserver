// --- Home page: click-to-cycle greetings ---
const greetings = [
  "Hello, World!",
  "Hola, Mundo!",
  "Bonjour le Monde!",
  "Hallo, Welt!",
  "Ciao, Mondo!",
  "你好，世界！",
  "こんにちは世界！",
];

const button = document.getElementById("clickBtn");
const greeting = document.getElementById("greeting");
let clickCount = 0;

if (button && greeting) {
  button.addEventListener("click", () => {
    clickCount++;
    greeting.textContent = greetings[clickCount % greetings.length];
    button.textContent = `Clicked ${clickCount} time${clickCount === 1 ? "" : "s"}`;
    greeting.style.color = "#764ba2";
    setTimeout(() => {
      greeting.style.color = "#333";
    }, 200);
  });
}

// --- About page: track server request count via sessionStorage ---
if (window.location.pathname === "/about") {
  let visits = parseInt(sessionStorage.getItem("aboutVisits") || "0", 10);
  visits++;
  sessionStorage.setItem("aboutVisits", visits);

  const countEl = document.getElementById("requestCount");
  if (countEl) {
    countEl.textContent = visits;
  }
}

// --- 404 page: show the bad path ---
const badPathEl = document.getElementById("badPath");
if (badPathEl) {
  badPathEl.textContent = window.location.pathname;
}

// --- Bad link generator ---
const badLinkBtn = document.getElementById("badLinkBtn");
if (badLinkBtn) {
  badLinkBtn.addEventListener("click", () => {
    const slug = Math.random().toString(36).substring(2, 10);
    window.location.href = "/" + slug;
  });
}

console.log(`Hello from script.js — you're on ${window.location.pathname}!`);
