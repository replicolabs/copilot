'use strict';

const CHAR_DELAY        = 30;
const ERASE_DELAY       = 16;
const HOLD_DURATION     = 2800;
const PAUSE_AFTER_ERASE = 500;

const PROMPTS = [
  'send a bundle and break down how it landed',
  'find out why my last bundle failed',
  "monitor the network and pick the best window",
  'check the tip floor before I send anything',
];

const inputText = document.getElementById('ccInputText');
const caret     = document.getElementById('ccCaret');

const prefersReducedMotion =
  window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function typeText(text) {
  for (const char of text) {
    inputText.textContent += char;
    await sleep(CHAR_DELAY);
  }
}

async function eraseText() {
  while (inputText.textContent.length > 0) {
    inputText.textContent = inputText.textContent.slice(0, -1);
    await sleep(ERASE_DELAY);
  }
}

let currentPrompt = 0;

async function animationLoop() {
  while (true) {
    await typeText(PROMPTS[currentPrompt]);
    await sleep(HOLD_DURATION);
    await eraseText();
    await sleep(PAUSE_AFTER_ERASE);
    currentPrompt = (currentPrompt + 1) % PROMPTS.length;
  }
}

if (inputText) {
  if (prefersReducedMotion) {
    inputText.textContent = PROMPTS[0];
    if (caret) caret.style.animation = 'none';
  } else {
    sleep(800).then(animationLoop);
  }
}

document.querySelectorAll('.copy-btn').forEach(btn => {
  btn.addEventListener('click', async () => {
    const text = btn.dataset.copy;
    if (!text) return;

    const copyIcon  = btn.querySelector('.copy-icon');
    const checkIcon = btn.querySelector('.check-icon');

    const showCopied = () => {
      copyIcon.classList.add('hidden');
      checkIcon.classList.remove('hidden');
      btn.classList.add('copied');
      setTimeout(() => {
        copyIcon.classList.remove('hidden');
        checkIcon.classList.add('hidden');
        btn.classList.remove('copied');
      }, 1800);
    };

    try {
      await navigator.clipboard.writeText(text);
      showCopied();
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); showCopied(); } catch {}
      ta.remove();
    }
  });
});