"use strict";
const $ = s => document.querySelector(s);
const call = (c, a) => window.__TAURI__.core.invoke(c, a);
const nf = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const pctTxt = p => (p > 0 && p < 0.1 ? "< 0,1" : nf.format(p)) + " %";
const fmt = n => { const u = ["o", "Ko", "Mo", "Go", "To"]; let i = 0; while (n >= 1024 && i < 4) { n /= 1024; i++; } return (i ? nf.format(n) : n) + " " + u[i]; };
const esc = s => String(s).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const isDark = () => matchMedia("(prefers-color-scheme: dark)").matches;
const TAGS = { dev: "Développeur", gamer: "Jeux", creative: "Créatif", general: "Général" };

const svg = d => `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;
const I = {
  chev: svg('<path d="m9 6 6 6-6 6"/>'),
  folder: svg('<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" fill="currentColor" fill-opacity=".25"/>'),
  file: svg('<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>'),
  lock: svg('<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>'),
};

const S = { root: null, rootPath: "", kids: new Map(), items: new Map(), parent: new Map(), names: new Map(), expanded: new Set(), sel: new Set(), cur: null, anchor: null, view: null, tm: null, rects: [], hover: null, vis: [], busy: false, ctx: null };

// --- utilitaires UI ------------------------------------------------------------

let toastTimer;
function toast(msg, kind = "") {
  const t = $("#toast");
  t.textContent = msg; t.className = "toast " + kind; t.hidden = false;
  clearTimeout(toastTimer); toastTimer = setTimeout(() => (t.hidden = true), 6000);
}
function busy(on, msg = "", sub = "") {
  S.busy = on; $("#busy").hidden = !on; $("#busy-msg").textContent = msg; $("#busy-sub").textContent = sub;
}
const guard = fn => async (...a) => { try { return await fn(...a); } catch (e) { busy(false); toast(String(e), "err"); } };

/** Retire les chemins dont un ancêtre est aussi présent (évite de supprimer deux fois). */
function dedupe(paths) {
  const set = new Set(paths);
  return paths.filter(p => {
    for (let i = p.length - 1; i > 0; i--) {
      if ((p[i] === "\\" || p[i] === "/") && set.has(p.slice(0, i))) return false;
    }
    return true;
  });
}

// --- chargement des données ----------------------------------------------------

async function loadKids(p) {
  const page = await call("children", { path: p, limit: 500 });
  S.kids.set(p, page);
  for (const it of page.items) { S.items.set(it.path, it); S.parent.set(it.path, p); S.names.set(it.path, it.name); }
}

const startScan = guard(async path => {
  if (S.busy) return;
  busy(true, "Analyse de " + path, "0 élément");
  const root = await call("scan", { path });
  Object.assign(S, {
    root, rootPath: root.path, kids: new Map(), items: new Map([[root.path, root]]), parent: new Map(),
    names: new Map([[root.path, root.name]]), expanded: new Set([root.path]), sel: new Set(), cur: null, anchor: null, view: null, tm: null, rects: [],
  });
  await loadKids(root.path);
  $("#home").hidden = true; $("#work").hidden = false;
  busy(false);
  await select(root.path, { drill: true });
});

async function loadTreemap() {
  const view = S.view;
  const tm = await call("treemap", { path: view });
  if (view !== S.view) return;
  const ingest = n => (n.children || []).forEach(c => { if (c.path) { S.parent.set(c.path, n.path); S.names.set(c.path, c.name); } ingest(c); });
  ingest(tm);
  S.tm = tm; relayout();
}

// --- arborescence --------------------------------------------------------------

function buildVis() {
  const v = [];
  const walk = (p, d) => {
    const pg = S.kids.get(p); if (!pg) return;
    const base = S.items.get(p).size || 1;
    for (const it of pg.items) { v.push({ it, d, base }); if (it.is_dir && S.expanded.has(it.path)) walk(it.path, d + 1); }
    if (pg.more) v.push({ more: pg.more, d });
  };
  v.push({ it: S.root, d: 0, base: S.root.size || 1 });
  if (S.expanded.has(S.rootPath)) walk(S.rootPath, 1);
  S.vis = v;
}

function rowHtml(v, i) {
  if (v.more) return `<div class="row more" style="--d:${v.d}"><span class="nm">… ${v.more.toLocaleString("fr-FR")} autres éléments plus petits</span></div>`;
  const it = v.it, pct = Math.min(100, (it.size * 100) / v.base);
  const cls = "row" + (S.sel.has(it.path) ? " sel" : "") + (S.cur === it.path ? " cur" : "");
  const chev = it.is_dir && it.files ? `<i class="chev${S.expanded.has(it.path) ? " open" : ""}">${I.chev}</i>` : "";
  return `<div class="${cls}" data-i="${i}"><span class="nm" style="--d:${v.d}"><span class="tw">${chev}</span><span class="ic ${it.is_dir ? "dir" : "file"}">${it.is_dir ? I.folder : I.file}</span><span class="t" title="${esc(it.path)}">${esc(it.name)}</span>${it.protected ? `<span class="lock" title="Protégé par DCleaner">${I.lock}</span>` : ""}</span><span class="sz">${fmt(it.size)}</span><span class="pc"><i><s style="width:${pct}%"></s></i><b>${pctTxt(pct)}</b></span><span class="fc">${it.is_dir ? it.files.toLocaleString("fr-FR") : ""}</span></div>`;
}

function renderRows() {
  buildVis();
  const box = $("#rows"), top = box.scrollTop;
  box.innerHTML = S.vis.map(rowHtml).join("");
  box.scrollTop = top;
}

const toggle = guard(async path => {
  if (S.expanded.has(path)) S.expanded.delete(path);
  else { if (!S.kids.has(path)) await loadKids(path); S.expanded.add(path); }
  renderRows();
});

async function reveal(path) {
  const chain = [];
  for (let p = S.parent.get(path); p && p !== S.rootPath; p = S.parent.get(p)) chain.unshift(p);
  chain.unshift(S.rootPath);
  for (const a of chain) { if (!S.kids.has(a)) await loadKids(a); S.expanded.add(a); }
}

const select = guard(async (path, o = {}) => {
  if (!S.items.has(path)) S.items.set(path, await call("info", { path }));
  const mode = o.mode || "one";
  if (mode === "range" && S.anchor) {
    const a = S.vis.findIndex(v => v.it && v.it.path === S.anchor), b = S.vis.findIndex(v => v.it && v.it.path === path);
    if (a >= 0 && b >= 0) S.sel = new Set(S.vis.slice(Math.min(a, b), Math.max(a, b) + 1).filter(v => v.it).map(v => v.it.path));
  } else if (mode === "toggle") {
    S.sel.has(path) ? S.sel.delete(path) : S.sel.add(path); S.anchor = path;
  } else { S.sel = new Set([path]); S.anchor = path; }
  S.cur = path;
  if (o.drill && mode === "one") {
    const it = S.items.get(path);
    const view = it.is_dir ? path : S.parent.get(path);
    if (view && view !== S.view) { S.view = view; renderCrumbs(); await loadTreemap(); }
  }
  renderAll();
  $("#rows .row.cur")?.scrollIntoView({ block: "nearest" });
});

function renderAll() { renderRows(); renderInfo(); drawMap(); updateButtons(); }

function renderCrumbs() {
  const chain = [];
  for (let p = S.view; p; p = S.parent.get(p)) { chain.unshift(p); if (p === S.rootPath) break; }
  $("#crumbs").innerHTML = chain.map((p, i) => `${i ? "<i>›</i>" : ""}<button data-p="${esc(p)}">${esc(S.names.get(p) || p)}</button>`).join("");
}

// --- treemap -------------------------------------------------------------------

const cv = $("#cv"), cx = cv.getContext("2d");
let W = 0, H = 0;

function relayout() {
  const r = cv.getBoundingClientRect(), dpr = devicePixelRatio || 1;
  W = r.width; H = r.height;
  cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr);
  cx.setTransform(dpr, 0, 0, dpr, 0, 0);
  S.rects = S.tm ? TM.layout(S.tm, W, H) : [];
  drawMap();
}

function drawMap() {
  if (!W) return;
  TM.draw(cx, W, H, S.rects, { sel: S.sel, hover: S.hover, dark: isDark(), fmt });
  if (S.tm && !S.rects.length) {
    cx.fillStyle = getComputedStyle(document.body).color; cx.globalAlpha = .5; cx.textAlign = "center";
    cx.fillText("Dossier vide", W / 2, H / 2); cx.textAlign = "start"; cx.globalAlpha = 1;
  }
}

new ResizeObserver(relayout).observe($("#cvwrap"));
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", drawMap);

$("#legend").innerHTML = Object.values(TM.CATS).map(([h, n]) => `<span><i style="background:hsl(${h} 50% 48%)"></i>${n}</span>`).join("");

const mapHit = e => { const b = cv.getBoundingClientRect(); return TM.hit(S.rects, e.clientX - b.left, e.clientY - b.top); };

cv.addEventListener("mousemove", e => {
  const r = mapHit(e), p = r && r.node.path ? r.node.path : null, tip = $("#tip");
  if (p !== S.hover) { S.hover = p; drawMap(); }
  cv.style.cursor = p ? "pointer" : "default";
  if (!r) { tip.hidden = true; return; }
  const n = r.node, pct = S.tm.size ? (n.size * 100) / S.tm.size : 0;
  tip.innerHTML = `<b>${esc(n.name)}</b>${fmt(n.size)} · ${nf.format(pct)} % de ce dossier${n.path ? `<br><span>${esc(n.path)}</span>` : ""}`;
  tip.hidden = false;
  const b = $("#cvwrap").getBoundingClientRect();
  tip.style.left = Math.max(0, Math.min(e.clientX - b.left + 14, b.width - 370)) + "px";
  tip.style.top = Math.max(0, Math.min(e.clientY - b.top + 14, b.height - 90)) + "px";
});
cv.addEventListener("mouseleave", () => { S.hover = null; $("#tip").hidden = true; drawMap(); });
cv.addEventListener("click", async e => {
  const r = mapHit(e);
  if (r && r.node.path) { await guard(reveal)(r.node.path); select(r.node.path, { mode: e.ctrlKey ? "toggle" : "one" }); }
});
cv.addEventListener("dblclick", async e => {
  const r = mapHit(e);
  if (r && r.node.path) { await guard(reveal)(r.node.path); select(r.node.path, { drill: true }); }
});
cv.addEventListener("contextmenu", async e => {
  e.preventDefault();
  const r = mapHit(e);
  if (r && r.node.path) { await guard(reveal)(r.node.path); if (!S.sel.has(r.node.path)) await select(r.node.path); showCtx(e, r.node.path); }
});

// --- infos & boutons -----------------------------------------------------------

function renderInfo() {
  const el = $("#info");
  const sel = [...S.sel].map(p => S.items.get(p)).filter(Boolean);
  if (sel.length > 1) {
    const total = dedupe(sel.map(i => i.path)).reduce((a, p) => a + S.items.get(p).size, 0);
    el.innerHTML = `<div class="i-main"><div class="i-title">${sel.length} éléments sélectionnés</div><div class="i-path">Ctrl ou Maj + clic pour modifier la sélection</div></div><div class="i-stats"><b>${fmt(total)}</b><span>au total</span></div>`;
    return;
  }
  const it = S.items.get(S.cur);
  if (!it) { el.innerHTML = ""; return; }
  const pct = S.root.size ? (it.size * 100) / S.root.size : 0;
  el.innerHTML = `<div class="i-main"><div class="i-title">${it.is_dir ? I.folder : I.file}${esc(it.name)}${it.protected ? `<span class="badge">${I.lock}Protégé</span>` : ""}</div><div class="i-path" title="${esc(it.path)}">${esc(it.path)}</div>${it.explain ? `<div class="i-expl">${esc(it.explain)}</div>` : ""}</div>
    <div class="i-stats"><b>${fmt(it.size)}</b><span>${nf.format(pct)} % de l'analyse${it.is_dir ? " · " + it.files.toLocaleString("fr-FR") + " fichiers" : ""}</span></div>
    <div class="i-act"><button class="btn" data-act="reveal">Afficher dans l'Explorateur</button><button class="btn" data-act="copy">Copier le chemin</button></div>`;
}

function updateButtons() {
  const has = !!S.root, trashable = [...S.sel].some(p => S.items.get(p) && !S.items.get(p).protected);
  $("#btn-up").disabled = !has || S.view === S.rootPath;
  $("#btn-caches").disabled = $("#btn-dupes").disabled = $("#btn-rescan").disabled = !has;
  $("#btn-trash").disabled = !trashable;
}

// --- actions -------------------------------------------------------------------

const openFolder = guard(async () => {
  const p = await window.__TAURI__.dialog.open({ directory: true, title: "Dossier ou lecteur à analyser" });
  if (p) startScan(p);
});

const showCaches = guard(async (path = S.view) => {
  const r = await call("caches", { path });
  if (!r.rows.length) return toast("Aucun résidu connu dans ce dossier.");
  confirmTrash({ title: "Résidus connus", note: `${r.rows.length} dossier(s) de cache dans <b>${esc(S.names.get(path) || path)}</b>. Les outils les recréent au prochain build ou lancement.`, rows: r.rows, skipped: r.skipped });
});

const showDupes = guard(async (path = S.view) => {
  busy(true, "Recherche de doublons…", "Comparaison par taille puis empreinte XXH3 (fichiers ≥ 1 Mo)");
  let r;
  try { r = await call("dupes", { path, minSize: 1 << 20 }); } finally { busy(false); }
  if (!r.rows.length) return toast("Aucun doublon (≥ 1 Mo) dans ce dossier.");
  confirmTrash({ title: "Doublons", note: `Dans chaque groupe de fichiers identiques, le premier (ordre alphabétique) est <b>conservé</b>. Seuls les autres sont listés.`, rows: r.rows, skipped: r.skipped });
});

function trashSelection() {
  const items = [...S.sel].map(p => S.items.get(p)).filter(Boolean);
  const rows = items.filter(i => !i.protected).map(i => ({ path: i.path, size: i.size, detail: i.explain || (i.is_dir ? "Dossier" : "Fichier"), tag: "" }));
  if (!rows.length) return;
  confirmTrash({ title: "Mettre à la Corbeille", note: "", rows, skipped: items.length - rows.length });
}

/** Fenêtre de simulation : liste exacte + espace libéré, rien n'est supprimé avant « Mettre à la Corbeille ». */
function confirmTrash({ title, note, rows, skipped }) {
  const dlg = $("#dlg");
  let filter = "*";
  rows = rows.map(r => ({ ...r, on: true })).sort((a, b) => b.size - a.size);
  const tags = [...new Set(rows.map(r => r.tag).filter(Boolean))];
  const shown = () => rows.filter(r => filter === "*" || r.tag === filter);
  const picked = () => { const ps = shown().filter(r => r.on); const keep = new Set(dedupe(ps.map(r => r.path))); return ps.filter(r => keep.has(r.path)); };
  const draw = () => {
    const list = shown(), cap = 1500;
    $("#d-list").innerHTML = list.slice(0, cap).map(r => `<div class="d-row" data-p="${esc(r.path)}"><input type="checkbox" ${r.on ? "checked" : ""}><span class="sz">${fmt(r.size)}</span><div><div class="p">${esc(r.path)}${r.tag ? `<span class="tg">${TAGS[r.tag] || r.tag}</span>` : ""}</div><div class="why">${esc(r.detail)}</div></div></div>`).join("")
      + (list.length > cap ? `<div class="d-row"><span></span><span></span><span class="why">… ${list.length - cap} autres plus petits (inclus dans le total)</span></div>` : "");
    const p = picked();
    $("#d-count").textContent = `${p.length} / ${list.length} sélectionné(s)`;
    $("#d-sum").textContent = `${fmt(p.reduce((a, r) => a + r.size, 0))} seront libérés`;
    $("#d-ok").disabled = !p.length;
    $("#d-all").checked = list.length > 0 && list.every(r => r.on);
  };
  $("#d-title").textContent = title;
  $("#d-note").innerHTML = `<b>Aperçu.</b> Rien n'est supprimé tant que vous n'avez pas confirmé. Les éléments vont dans la Corbeille et restent restaurables.` + (note ? "<br>" + note : "") + (skipped ? `<br>${skipped} élément(s) protégé(s) du système exclu(s) de la liste.` : "");
  $("#d-chips").hidden = tags.length < 2;
  $("#d-chips").innerHTML = ["*", ...tags].map(t => `<button data-t="${t}" class="${t === "*" ? "on" : ""}">${t === "*" ? "Tous" : TAGS[t] || t}</button>`).join("");
  $("#d-chips").onclick = e => { const b = e.target.closest("button"); if (!b) return; filter = b.dataset.t; [...$("#d-chips").children].forEach(c => c.classList.toggle("on", c === b)); draw(); };
  $("#d-list").onclick = e => { const el = e.target.closest(".d-row[data-p]"); if (!el) return; const r = rows.find(x => x.path === el.dataset.p); r.on = !r.on; draw(); };
  $("#d-all").onchange = e => { shown().forEach(r => (r.on = e.target.checked)); draw(); };
  $("#d-cancel").onclick = () => dlg.close();
  $("#d-ok").onclick = () => { const paths = picked().map(r => r.path); dlg.close(); doTrash(paths); };
  draw();
  dlg.showModal();
}

const doTrash = guard(async paths => {
  busy(true, "Envoi à la Corbeille…", `${paths.length} élément(s)`);
  let r;
  try { r = await call("trash_paths", { paths }); } finally { busy(false); }
  toast(`${r.removed} élément(s) envoyé(s) à la Corbeille, ${fmt(r.freed)} libérés.` + (r.failed ? ` ${r.failed} échec(s). ${r.error || ""}${S.admin ? "" : " Si l'accès est refusé, utilisez « Relancer en administrateur »."}` : ""), r.removed ? "ok" : "err");
  await refresh();
});

async function exists(p) { try { S.items.set(p, await call("info", { path: p })); return true; } catch { return false; } }

async function refresh() {
  S.kids.clear();
  await Promise.all([...S.expanded].map(p => loadKids(p).catch(() => S.expanded.delete(p))));
  S.root = await call("info", { path: S.rootPath }); S.items.set(S.rootPath, S.root);
  for (const p of [...S.sel]) if (!(await exists(p))) S.sel.delete(p);
  while (S.view !== S.rootPath && !(await exists(S.view))) S.view = S.parent.get(S.view) || S.rootPath;
  if (S.cur && !S.sel.has(S.cur)) S.cur = null;
  renderCrumbs(); await loadTreemap(); renderAll();
}

function up() { const p = S.parent.get(S.view); if (p) select(p, { drill: true }); }

// --- menu contextuel -----------------------------------------------------------

function showCtx(e, path) {
  const it = S.items.get(path); if (!it) return;
  S.ctx = path;
  const m = $("#ctx");
  m.innerHTML = [
    it.is_dir && `<button data-a="">Analyser ce dossier</button>`,
    `<button data-a="">Ouvrir dans l'Explorateur</button>`,
    it.is_dir && `<button data-a="">Résidus connus ici</button>`,
    it.is_dir && `<button data-a="">Doublons ici</button>`,
    `<button data-a="">Copier le chemin</button>`, "<hr>",
    `<button data-a="trash" ${it.protected ? "disabled" : ""}>Mettre à la Corbeille…</button>`,
  ].filter(Boolean).join("");
  m.hidden = false;
  m.style.left = Math.min(e.clientX, innerWidth - 250) + "px";
  m.style.top = Math.min(e.clientY, innerHeight - m.offsetHeight - 8) + "px";
}

function act(a, path) {
  $("#ctx").hidden = true;
  const it = S.items.get(path);
  if (a === "scan") startScan(path);
  else if (a === "reveal") call("reveal", { path });
  else if (a === "caches") showCaches(path);
  else if (a === "dupes") showDupes(path);
  else if (a === "copy") navigator.clipboard.writeText(path).then(() => toast("Chemin copié."));
  else if (a === "trash") trashSelection();
}

document.addEventListener("click", e => { if (!e.target.closest("#ctx")) $("#ctx").hidden = true; });
document.addEventListener("keydown", e => { if (e.key === "Escape") $("#ctx").hidden = true; });
$("#ctx").addEventListener("click", e => { const b = e.target.closest("button"); if (b && !b.disabled) act(b.dataset.a, S.ctx); });
$("#info").addEventListener("click", e => { const b = e.target.closest("[data-act]"); if (b && S.cur) act(b.dataset.act, S.cur); });

// --- événements ----------------------------------------------------------------

$("#rows").addEventListener("click", e => {
  const r = e.target.closest(".row[data-i]"); if (!r) return;
  const v = S.vis[r.dataset.i];
  if (e.target.closest(".chev")) return toggle(v.it.path);
  select(v.it.path, { mode: e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "one", drill: true });
});
$("#rows").addEventListener("dblclick", e => { const r = e.target.closest(".row[data-i]"); if (r && S.vis[r.dataset.i].it.is_dir) toggle(S.vis[r.dataset.i].it.path); });
$("#rows").addEventListener("contextmenu", async e => {
  const r = e.target.closest(".row[data-i]"); if (!r) return;
  e.preventDefault();
  const p = S.vis[r.dataset.i].it.path;
  if (!S.sel.has(p)) await select(p);
  showCtx(e, p);
});
$("#tree").addEventListener("keydown", e => {
  const i = S.vis.findIndex(v => v.it && v.it.path === S.cur); if (i < 0) return;
  const it = S.vis[i].it;
  const go = j => { const v = S.vis[j]; if (v && v.it) select(v.it.path, { mode: e.shiftKey ? "range" : "one", drill: !e.shiftKey }); };
  if (e.key === "ArrowDown") { go(i + 1); e.preventDefault(); }
  else if (e.key === "ArrowUp") { go(i - 1); e.preventDefault(); }
  else if (e.key === "ArrowRight") { if (it.is_dir && !S.expanded.has(it.path)) toggle(it.path); else go(i + 1); e.preventDefault(); }
  else if (e.key === "ArrowLeft") { if (it.is_dir && S.expanded.has(it.path) && it.path !== S.rootPath) toggle(it.path); else if (S.parent.get(it.path)) select(S.parent.get(it.path), { drill: true }); e.preventDefault(); }
  else if (e.key === "Delete") trashSelection();
});
$("#crumbs").addEventListener("click", e => { const b = e.target.closest("button"); if (b) select(b.dataset.p, { drill: true }); });

$("#btn-open").onclick = openFolder;
$("#btn-rescan").onclick = () => startScan(S.rootPath);
$("#btn-up").onclick = up;
$("#btn-admin").onclick = guard(() => call("relaunch_admin", { path: S.rootPath || null }));
$("#btn-caches").onclick = () => showCaches();
$("#btn-dupes").onclick = () => showDupes();
$("#btn-trash").onclick = trashSelection;
$("#brand").onclick = () => { if (S.root) { $("#home").hidden = !$("#home").hidden; $("#work").hidden = !$("#home").hidden; relayout(); } };

// séparateur arbre / treemap
$("#gutter").addEventListener("pointerdown", e => {
  const g = e.currentTarget; g.setPointerCapture(e.pointerId);
  const move = ev => { const w = $("#work").getBoundingClientRect(); $("#work").style.setProperty("--left", Math.max(320, Math.min(ev.clientX - w.left, w.width - 320)) + "px"); };
  g.addEventListener("pointermove", move);
  g.addEventListener("pointerup", () => g.removeEventListener("pointermove", move), { once: true });
});

// --- démarrage -----------------------------------------------------------------

async function showDrives() {
  const ds = await call("drives");
  $("#drives").innerHTML = ds.map((d, i) => {
    const pct = ((d.total - d.free) * 100) / d.total;
    return `<button class="vol-row" data-i="${i}"><b>${esc(d.mount)}${d.name ? " · " + esc(d.name) : ""}</b><div class="meter ${pct > 90 ? "hot" : ""}" title="${nf.format(pct)} % utilisé"><i style="width:${pct}%"></i></div><small>${fmt(d.free)} libres sur ${fmt(d.total)}</small></button>`;
  }).join("");
  $("#drives").onclick = e => { const b = e.target.closest(".vol-row"); if (b) startScan(ds[b.dataset.i].mount); };
}

async function main() {
  updateButtons();
  S.admin = await call("is_admin");
  $("#adm").hidden = !S.admin; $("#btn-admin").hidden = S.admin;
  window.__TAURI__.event.listen("scan-progress", e => { if (S.busy) $("#busy-sub").textContent = e.payload.toLocaleString("fr-FR") + " éléments"; });
  showDrives().catch(() => {});
  const p = await call("initial_path");
  if (p) startScan(p);
}
main();
