// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT
//
// The web view: read-only panes over FastTail's streams. Log text is only ever set with
// textContent; the styles come from the server as CSS declarations per class.
'use strict';

const MAX_ROWS = 5000;
const state = { texts: {}, streams: [], css: new Set(), follow: true, panes: [] };

function t(key, fallback) { return state.texts[key] || fallback; }

async function api(path) {
  const r = await fetch(path, { credentials: 'same-origin', cache: 'no-store' });
  if (!r.ok) throw new Error(String(r.status));
  return r.json();
}

function sheet() {
  for (const s of document.styleSheets) {
    if (s.href && s.href.endsWith('/base.css')) return s;
  }
  return document.styleSheets[0];
}

function addCss(css) {
  if (!css) return;
  const s = sheet();
  for (const [cls, rule] of Object.entries(css)) {
    if (state.css.has(cls)) continue;
    state.css.add(cls);
    try { s.insertRule('.rows .' + cls + '{' + rule + '}', s.cssRules.length); } catch (e) { /* a rule the browser refuses */ }
  }
}

function rowElement(row) {
  const div = document.createElement('div');
  div.className = 'l';
  if (row.c !== null && row.c !== undefined) div.classList.add('c' + row.c);
  div.dataset.row = String(row.r);
  const n = document.createElement('span');
  n.className = 'n';
  n.textContent = String(row.n);
  div.appendChild(n);
  if (row.b) {
    const b = document.createElement('span');
    b.className = 'bm';
    b.textContent = '★ ';
    div.appendChild(b);
  }
  for (const [text, cls] of row.t) {
    if (cls === null || cls === undefined) {
      div.appendChild(document.createTextNode(text));
    } else {
      const s = document.createElement('span');
      s.className = 'c' + cls;
      s.textContent = text;
      div.appendChild(s);
    }
  }
  markHit(div);
  return div;
}

function markHit(div) {
  const q = document.getElementById('query').value.trim().toLowerCase();
  div.classList.toggle('hit', q.length > 0 && div.textContent.toLowerCase().includes(q));
}

class Pane {
  constructor(el) {
    this.el = el;
    this.rows = el.querySelector('.rows');
    this.note = el.querySelector('.note');
    this.earlierBtn = el.querySelector('.earlier');
    this.earlierBtn.addEventListener('click', () => this.earlier());
    // At the top, the page before loads.
    el.addEventListener('scroll', () => { if (el.scrollTop === 0 && this.first > 0) this.earlier(); });
    this.id = null; this.es = null; this.first = 0; this.next = 0; this.gen = 0; this.loading = false;
  }

  close() {
    if (this.es) { this.es.close(); this.es = null; }
  }

  async open(id, from) {
    this.close();
    this.id = id;
    this.rows.replaceChildren();
    this.note.textContent = '';
    if (!id) return;
    const q = '/api/lines?s=' + encodeURIComponent(id) + (from === undefined ? '' : '&from=' + from);
    let page;
    try { page = await api(q); } catch (e) { this.note.textContent = t('web_gone', 'The stream was closed.'); return; }
    addCss(page.css);
    this.first = page.first; this.next = page.next; this.gen = page.gen;
    const frag = document.createDocumentFragment();
    for (const row of page.rows) frag.appendChild(rowElement(row));
    this.rows.appendChild(frag);
    this.earlierBtn.disabled = this.first === 0;
    if (from === undefined) this.toBottom();
    this.listen();
  }

  listen() {
    const url = '/api/events?s=' + encodeURIComponent(this.id) + '&after=' + this.next + '&gen=' + this.gen;
    this.es = new EventSource(url);
    this.es.addEventListener('lines', (e) => this.append(JSON.parse(e.data)));
    this.es.addEventListener('reset', () => this.open(this.id));
    this.es.addEventListener('gone', () => { this.close(); this.note.textContent = t('web_gone', 'The stream was closed.'); });
  }

  append(batch) {
    addCss(batch.css);
    if (batch.first !== this.next) { this.open(this.id); return; }
    const frag = document.createDocumentFragment();
    for (const row of batch.rows) frag.appendChild(rowElement(row));
    this.rows.appendChild(frag);
    this.next = batch.next; this.gen = batch.gen;
    while (this.rows.childElementCount > MAX_ROWS) {
      this.rows.firstElementChild.remove();
      this.first += 1;
    }
    this.earlierBtn.disabled = this.first === 0;
    if (state.follow) this.toBottom();
  }

  async earlier() {
    if (this.first === 0 || this.loading) return;
    this.loading = true;
    const from = Math.max(0, this.first - 500);
    let page;
    try {
      page = await api('/api/lines?s=' + encodeURIComponent(this.id) + '&from=' + from + '&count=' + (this.first - from));
    } finally {
      this.loading = false;
    }
    addCss(page.css);
    const before = this.el.scrollHeight;
    const frag = document.createDocumentFragment();
    for (const row of page.rows) frag.appendChild(rowElement(row));
    this.rows.prepend(frag);
    this.first = page.first;
    this.el.scrollTop += this.el.scrollHeight - before;
    this.earlierBtn.disabled = this.first === 0;
  }

  toBottom() { this.el.scrollTop = this.el.scrollHeight; }

  async find(forward) {
    const q = document.getElementById('query').value.trim();
    if (!q || !this.id) return;
    const cur = this.rows.querySelector('.cur');
    const at = cur ? Number(cur.dataset.row) : (forward ? this.first - 1 : this.next);
    const res = await api('/api/search?s=' + encodeURIComponent(this.id) + '&row=' + at + '&dir=' + (forward ? 'next' : 'prev') + '&q=' + encodeURIComponent(q));
    const found = document.getElementById('found');
    if (res.row === null || res.row === undefined) { found.textContent = t('web_no_match', 'no match'); return; }
    found.textContent = '';
    // The match stays on screen: new lines no longer scroll it away.
    setFollow(false);
    let el = this.rows.querySelector('[data-row="' + res.row + '"]');
    if (!el) {
      await this.open(this.id, Math.max(0, res.row - 250));
      el = this.rows.querySelector('[data-row="' + res.row + '"]');
    }
    if (cur) cur.classList.remove('cur');
    if (el) { el.classList.add('cur'); el.scrollIntoView({ block: 'center' }); }
  }
}

function setFollow(on) {
  state.follow = on;
  const b = document.getElementById('follow');
  b.textContent = on ? t('web_pause', 'Pause') : t('web_follow', 'Follow');
  b.classList.toggle('on', !on);
  if (on) for (const p of state.panes) p.toBottom();
}

function fillSelect(sel, chosen) {
  sel.replaceChildren();
  for (const s of state.streams) {
    const o = document.createElement('option');
    o.value = s.id;
    o.textContent = s.name;
    sel.appendChild(o);
  }
  if (chosen && state.streams.some((s) => s.id === chosen)) sel.value = chosen;
}

async function start() {
  const info = await api('/api/streams');
  state.texts = info.texts || {};
  state.streams = info.streams || [];
  const root = sheet();
  try { root.insertRule(':root{' + info.theme + '}', root.cssRules.length); } catch (e) { /* keep the defaults */ }
  for (const el of document.querySelectorAll('[data-t]')) el.textContent = t(el.dataset.t, el.textContent);
  for (const el of document.querySelectorAll('[data-ph]')) el.placeholder = t(el.dataset.ph, el.placeholder);
  document.getElementById('status').textContent = t('web_streams', '{0} streams').replace('{0}', String(state.streams.length));
  state.panes = [new Pane(document.getElementById('pane-a')), new Pane(document.getElementById('pane-b'))];
  const a = document.getElementById('stream-a');
  const b = document.getElementById('stream-b');
  fillSelect(a); fillSelect(b, state.streams[1] && state.streams[1].id);
  a.addEventListener('change', () => state.panes[0].open(a.value));
  b.addEventListener('change', () => state.panes[1].open(b.value));
  const two = document.getElementById('two');
  two.addEventListener('change', () => {
    document.getElementById('pane-b').classList.toggle('hidden', !two.checked);
    if (two.checked) state.panes[1].open(b.value); else state.panes[1].close();
  });
  document.getElementById('follow').addEventListener('click', () => setFollow(!state.follow));
  const query = document.getElementById('query');
  query.addEventListener('input', () => { for (const el of document.querySelectorAll('.rows .l')) markHit(el); });
  query.addEventListener('keydown', (e) => { if (e.key === 'Enter') state.panes[0].find(!e.shiftKey); });
  document.getElementById('next').addEventListener('click', () => state.panes[0].find(true));
  document.getElementById('prev').addEventListener('click', () => state.panes[0].find(false));
  for (const p of state.panes) {
    p.el.addEventListener('wheel', (e) => { if (e.deltaY < 0 && state.follow) setFollow(false); });
  }
  setFollow(true);
  if (state.streams.length === 0) {
    state.panes[0].note.textContent = t('web_no_streams', 'No stream is open in FastTail.');
  } else {
    state.panes[0].open(a.value);
  }
}

start().catch(() => {
  document.getElementById('status').textContent = 'FastTail is not answering.';
});
