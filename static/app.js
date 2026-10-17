const state = {
  view: "search",
  offset: 0,
  total: 0,
  hits: [],
  loading: false,
  selected: {},
  pack: null,
};

const els = {
  form: document.getElementById("search-form"),
  query: document.getElementById("query"),
  source: document.getElementById("source"),
  type: document.getElementById("type"),
  loader: document.getElementById("loader"),
  version: document.getElementById("version"),
  loaderVersion: document.getElementById("loader-version"),
  versionList: document.getElementById("version-list"),
  loaderVersionList: document.getElementById("loader-version-list"),
  sort: document.getElementById("sort"),
  results: document.getElementById("results"),
  meta: document.getElementById("meta"),
  warnings: document.getElementById("warnings"),
  more: document.getElementById("more"),
  stats: document.getElementById("stats"),
  settings: document.getElementById("settings-panel"),
  keyForm: document.getElementById("key-form"),
  apiKey: document.getElementById("api-key"),
  keyStatus: document.getElementById("key-status"),
  drawer: document.getElementById("drawer"),
  drawerBody: document.getElementById("drawer-body"),
  backdrop: document.getElementById("backdrop"),
  packDock: document.getElementById("pack-dock"),
  packCount: document.getElementById("pack-count"),
  packTarget: document.getElementById("pack-target"),
  packPanel: document.getElementById("pack-panel"),
  packIssues: document.getElementById("pack-issues"),
  packFiles: document.getElementById("pack-files"),
  packCheck: document.getElementById("pack-check"),
  packDownload: document.getElementById("pack-download"),
  packClear: document.getElementById("pack-clear"),
};

function params() {
  return {
    q: els.query.value.trim(),
    source: els.source.value,
    type: els.type.value,
    loader: els.loader.value,
    version: els.version.value.trim(),
    sort: els.sort.value,
  };
}

function packPayload() {
  return {
    game_version: els.version.value.trim(),
    loader: els.loader.value,
    loader_version: els.loaderVersion.value.trim(),
    mods: Object.values(state.selected),
  };
}

function modKey(mod) {
  return `${mod.source}:${mod.source_id}`;
}

function fmt(n) {
  n = Number(n) || 0;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function esc(value) {
  return String(value ?? "").replace(/[&<>"']/g, (ch) => (
    { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[ch]
  ));
}

function icon(mod) {
  if (mod.icon_url) return `<img src="${esc(mod.icon_url)}" alt="" />`;
  return `<div class="ph">${esc((mod.name || "?").slice(0, 1))}</div>`;
}

function chips(mod) {
  const tags = [
    `<span class="badge ${esc(mod.source)}">${esc(mod.source)}</span>`,
    ...(mod.loaders || []).slice(0, 3).map((l) => `<span class="tag">${esc(l)}</span>`),
    ...(mod.game_versions || []).slice(0, 3).map((v) => `<span class="tag">${esc(v)}</span>`),
  ];
  return tags.join("");
}

function card(mod) {
  const key = modKey(mod);
  const on = Boolean(state.selected[key]);
  return `
    <article class="card ${on ? "selected" : ""}" data-source="${esc(mod.source)}" data-id="${esc(mod.source_id)}">
      <label class="pick" data-pick><input type="checkbox" ${on ? "checked" : ""} /></label>
      ${icon(mod)}
      <div>
        <h3>${esc(mod.name)}</h3>
        <div class="author">${esc(mod.author || "Unknown author")}</div>
        <p>${esc(mod.summary || "")}</p>
      </div>
      <button class="star ${mod.favorite ? "on" : ""}" data-star title="Save">★</button>
      <div class="tags">${chips(mod)}</div>
      <div class="stats-row">${fmt(mod.downloads)} downloads</div>
    </article>
  `;
}

function renderDock() {
  const n = Object.keys(state.selected).length;
  els.packDock.hidden = n === 0 || state.view === "settings";
  els.packCount.textContent = `${n} mod${n === 1 ? "" : "s"} selected`;
  const target = [els.loader.value, els.loaderVersion.value.trim(), els.version.value.trim()]
    .filter(Boolean)
    .join(" · ");
  els.packTarget.textContent = target || "Set Minecraft + loader for a precise check";
  if (!n) els.packPanel.hidden = true;
}

function renderPack(report) {
  state.pack = report;
  els.packPanel.hidden = false;
  const errors = (report.issues || []).filter((i) => i.severity === "error");
  const banner = report.ok
    ? `<p class="ok-banner">No blocking incompatibilities for ${esc(report.loader || "the selected loader")} ${esc(report.game_version || "")}.</p>`
    : `<p class="issue error">${errors.length} compatibility problem${errors.length === 1 ? "" : "s"} — download anyway only if you know it is safe.</p>`;
  els.packIssues.innerHTML = banner + (report.issues || []).map((issue) => (
    `<div class="issue ${esc(issue.severity)}">${esc(issue.message)}</div>`
  )).join("");
  els.packFiles.innerHTML = (report.resolved || []).map((file) => `
    <div class="file">
      <strong>${esc(file.name)}</strong>
      <div class="stats-row">${esc(file.file_name || "")} · ${esc((file.game_versions || []).slice(0, 4).join(", "))} · ${esc((file.loaders || []).join(", "))}</div>
    </div>
  `).join("");
  els.packDownload.textContent = report.ok ? "Download" : "Download anyway";
}

function render(append = false) {
  if (!append) els.results.innerHTML = "";
  if (!state.hits.length && state.view !== "settings") {
    els.results.innerHTML = `<div class="empty"><strong>No mods yet</strong>Search CurseForge and Modrinth. Matches are cached in the local library.</div>`;
  } else if (state.view !== "settings") {
    els.results.insertAdjacentHTML("beforeend", state.hits.map(card).join(""));
  }
  els.meta.textContent = state.view === "settings" ? "" : `${state.total} result${state.total === 1 ? "" : "s"}`;
  els.more.hidden = state.view === "settings" || state.hits.length >= state.total || !state.hits.length;
  renderDock();
}

function findMod(source, id) {
  return state.hits.find((m) => m.source === source && String(m.source_id) === String(id))
    || state.selected[`${source}:${id}`];
}

function toggleSelect(mod, on) {
  const key = modKey(mod);
  if (on) {
    state.selected[key] = {
      source: mod.source,
      source_id: String(mod.source_id),
      name: mod.name,
      slug: mod.slug,
      loaders: mod.loaders || [],
      game_versions: mod.game_versions || [],
      icon_url: mod.icon_url,
      website_url: mod.website_url,
    };
  } else {
    delete state.selected[key];
  }
  state.pack = null;
  renderDock();
}

async function loadStatus() {
  const res = await fetch("/api/status");
  const data = await res.json();
  els.stats.innerHTML = `${data.stats.total} cached<br>${data.stats.modrinth} Modrinth · ${data.stats.curseforge} CurseForge<br>${data.stats.favorites} saved`;
  els.keyStatus.textContent = data.curseforge ? "CurseForge key is saved." : "No CurseForge key yet — Modrinth still works.";
}

async function loadVersions() {
  const res = await fetch("/api/versions");
  const data = await res.json();
  els.versionList.innerHTML = "";
  for (const version of data.versions.slice(0, 80)) {
    const opt = document.createElement("option");
    opt.value = version;
    els.versionList.append(opt);
  }
}

async function loadLoaderVersions() {
  const loader = els.loader.value;
  const version = els.version.value.trim();
  els.loaderVersionList.innerHTML = "";
  if (!loader) return;
  const query = new URLSearchParams({ loader, version });
  const res = await fetch(`/api/loader-versions?${query}`);
  const data = await res.json();
  for (const item of data.versions || []) {
    const opt = document.createElement("option");
    opt.value = item;
    els.loaderVersionList.append(opt);
  }
}

function endpoint() {
  if (state.view === "library") return "/api/library";
  if (state.view === "favorites") return "/api/favorites";
  return "/api/search";
}

async function search(reset = true) {
  if (state.view === "settings") return;
  if (reset) {
    state.offset = 0;
    state.hits = [];
  }
  state.loading = true;
  els.meta.textContent = "Searching…";
  const query = new URLSearchParams({ ...params(), offset: String(state.offset), limit: "20" });
  if (state.view !== "search") query.delete("source");
  const res = await fetch(`${endpoint()}?${query}`);
  const data = await res.json();
  state.total = data.total || 0;
  state.hits = reset ? (data.hits || []) : state.hits.concat(data.hits || []);
  els.warnings.textContent = (data.warnings || []).join(" ");
  render(!reset);
  state.loading = false;
  loadStatus();
}

function setView(view) {
  state.view = view;
  document.querySelectorAll(".nav-btn").forEach((btn) => btn.classList.toggle("active", btn.dataset.view === view));
  els.settings.hidden = view !== "settings";
  els.results.hidden = view === "settings";
  els.more.hidden = view === "settings";
  if (view === "library") els.source.value = "local";
  if (view === "search" && els.source.value === "local") els.source.value = "all";
  if (view !== "settings") search(true);
  else {
    els.meta.textContent = "";
    els.warnings.textContent = "";
    els.packDock.hidden = true;
    els.packPanel.hidden = true;
  }
}

async function openMod(source, id) {
  els.drawer.hidden = false;
  els.backdrop.hidden = false;
  els.drawerBody.innerHTML = "<p>Loading…</p>";
  const res = await fetch(`/api/mods/${source}/${id}`);
  const mod = await res.json();
  const files = (mod.files || []).map((file) => `
    <div class="file">
      <a href="${esc(file.url || mod.website_url)}" target="_blank" rel="noreferrer">${esc(file.name || "File")}</a>
      <div class="stats-row">${esc((file.game_versions || []).slice(0, 4).join(", "))} · ${esc((file.loaders || []).join(", "))}</div>
    </div>
  `).join("");
  els.drawerBody.innerHTML = `
    ${icon(mod)}
    <h2>${esc(mod.name)}</h2>
    <div class="author">${esc(mod.author || "")}</div>
    <div class="tags">${chips(mod)}</div>
    <p class="summary">${esc(mod.summary || "")}</p>
    <p><a href="${esc(mod.website_url)}" target="_blank" rel="noreferrer">Open on ${esc(mod.source)}</a></p>
    <div class="files">${files || "<p class='summary'>No files listed.</p>"}</div>
  `;
}

function closeDrawer() {
  els.drawer.hidden = true;
  els.backdrop.hidden = true;
}

async function checkPack() {
  if (!Object.keys(state.selected).length) return;
  els.packIssues.innerHTML = `<p class="issue info">Checking Minecraft version, loader, files, and declared incompatibilities…</p>`;
  els.packFiles.innerHTML = "";
  els.packPanel.hidden = false;
  const res = await fetch("/api/pack/check", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(packPayload()),
  });
  renderPack(await res.json());
}

async function downloadPack() {
  if (!Object.keys(state.selected).length) return;
  if (!state.pack) await checkPack();
  els.packDownload.disabled = true;
  els.packDownload.textContent = "Downloading…";
  try {
    const res = await fetch("/api/pack/download", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(packPayload()),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: "Download failed" }));
      els.packIssues.insertAdjacentHTML("afterbegin", `<div class="issue error">${esc(err.detail || "Download failed")}</div>`);
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    const label = [els.version.value.trim(), els.loader.value].filter(Boolean).join("-") || "mods";
    a.href = url;
    a.download = `moddb-${label}.zip`;
    a.click();
    URL.revokeObjectURL(url);
  } finally {
    els.packDownload.disabled = false;
    els.packDownload.textContent = state.pack && !state.pack.ok ? "Download anyway" : "Download";
  }
}

els.form.addEventListener("submit", (e) => {
  e.preventDefault();
  if (state.view === "settings") setView("search");
  search(true);
});
els.more.addEventListener("click", () => {
  state.offset += 20;
  search(false);
});
["source", "type", "loader", "sort"].forEach((id) => {
  document.getElementById(id).addEventListener("change", () => {
    if (id === "loader") loadLoaderVersions();
    renderDock();
    search(true);
  });
});
els.version.addEventListener("change", () => {
  loadLoaderVersions();
  renderDock();
  search(true);
});
els.loaderVersion.addEventListener("change", renderDock);
document.querySelectorAll(".nav-btn").forEach((btn) => {
  btn.addEventListener("click", () => setView(btn.dataset.view));
});
els.results.addEventListener("click", async (e) => {
  const pick = e.target.closest("[data-pick]");
  const star = e.target.closest("[data-star]");
  const cardEl = e.target.closest(".card");
  if (!cardEl) return;
  const mod = findMod(cardEl.dataset.source, cardEl.dataset.id);
  if (pick) {
    e.stopPropagation();
    const box = pick.matches("input") ? pick : pick.querySelector("input");
    toggleSelect(mod, box.checked);
    cardEl.classList.toggle("selected", box.checked);
    return;
  }
  if (star) {
    e.stopPropagation();
    const favorite = !star.classList.contains("on");
    await fetch(`/api/mods/${cardEl.dataset.source}/${cardEl.dataset.id}/favorite`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ favorite }),
    });
    star.classList.toggle("on", favorite);
    loadStatus();
    return;
  }
  openMod(cardEl.dataset.source, cardEl.dataset.id);
});
els.packCheck.addEventListener("click", checkPack);
els.packDownload.addEventListener("click", downloadPack);
els.packClear.addEventListener("click", () => {
  state.selected = {};
  state.pack = null;
  render(false);
});
document.getElementById("close-drawer").addEventListener("click", closeDrawer);
els.backdrop.addEventListener("click", closeDrawer);
els.keyForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  const res = await fetch("/api/settings/curseforge", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ api_key: els.apiKey.value }),
  });
  els.keyStatus.textContent = res.ok ? "CurseForge key saved." : "That API key was rejected.";
  if (res.ok) els.apiKey.value = "";
  loadStatus();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "/" && document.activeElement !== els.query) {
    e.preventDefault();
    els.query.focus();
  }
  if (e.key === "Escape") closeDrawer();
});

loadVersions();
loadLoaderVersions();
loadStatus();
search(true);
