/** Entry: DOM wiring, load, zoom, and refresh. */

import {
  state,
  ZOOM_MIN,
  ZOOM_MAX,
  ZOOM_STEP,
  headerH,
  cellSize,
  selectEntity,
  selectCell,
} from "./state.js";
import {
  parseData,
  buildTree,
  sortTree,
  setExpandLevel,
  collectVisible,
  rebuildOutgoing,
} from "./data.js";
import {
  initView,
  getRowCanvas,
  syncHeaderScroll,
  renderKindFilters,
  renderTree,
  renderRowHeader,
  renderColHeader,
  renderMatrix,
  updateStats,
  showCellHover,
  hideHoverCard,
  hitTest,
} from "./view.js";

const els = {
  status: document.getElementById("status"),
  fileInput: document.getElementById("fileInput"),
  loadDefault: document.getElementById("loadDefault"),
  search: document.getElementById("search"),
  kindFilters: document.getElementById("kindFilters"),
  stats: document.getElementById("stats"),
  tree: document.getElementById("tree"),
  viewport: document.getElementById("viewport"),
  canvas: document.getElementById("matrix"),
  hoverCard: document.getElementById("hoverCard"),
  colHeader: document.getElementById("colHeader"),
  rowHeader: document.getElementById("rowHeader"),
  sortGiven: document.getElementById("sortGiven"),
  sortName: document.getElementById("sortName"),
  sortLowerTri: document.getElementById("sortLowerTri"),
  levelFolders: document.getElementById("levelFolders"),
  levelFiles: document.getElementById("levelFiles"),
  levelSymbols: document.getElementById("levelSymbols"),
  zoomIn: document.getElementById("zoomIn"),
  zoomOut: document.getElementById("zoomOut"),
  zoomReset: document.getElementById("zoomReset"),
  labelResize: document.getElementById("labelResize"),
  matrixShell: document.getElementById("matrixShell"),
  kindModeToggle: document.getElementById("kindModeToggle"),
  kindModeOnly: document.getElementById("kindModeOnly"),
  kindReset: document.getElementById("kindReset"),
};

function updateZoomLabel() {
  els.zoomReset.textContent = `${Math.round(state.zoom * 100)}%`;
  document.documentElement.style.setProperty("--header", `${headerH()}px`);
}

function applyLabelWidth(w) {
  const shellW = els.matrixShell?.clientWidth || 800;
  state.labelW = Math.max(120, Math.min(Math.floor(shellW * 0.7), Math.round(w)));
  document.documentElement.style.setProperty("--label-w", `${state.labelW}px`);
}

function applyKindFilter() {
  rebuildOutgoing();
  if (state.sortMode === "lower-tri") sortTree();
  refresh(true);
}

function refresh(rebuildFilters = true) {
  if (!state.data) return;
  collectVisible();
  if (rebuildFilters) renderKindFilters(applyKindFilter);
  renderTree();
  renderColHeader();
  renderRowHeader();
  renderMatrix();
  updateStats();
  syncHeaderScroll();
}

function setZoom(next) {
  state.zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(next * 100) / 100));
  updateZoomLabel();
  refresh(false);
}

function setSort(mode) {
  state.sortMode = mode;
  els.sortGiven.classList.toggle("active", mode === "given");
  els.sortName.classList.toggle("active", mode === "name");
  els.sortLowerTri.classList.toggle("active", mode === "lower-tri");
  sortTree();
  refresh();
}

function setLevel(level) {
  els.levelFolders.classList.toggle("active", level === "folders");
  els.levelFiles.classList.toggle("active", level === "files");
  els.levelSymbols.classList.toggle("active", level === "symbols");
  setExpandLevel(level);
  refresh();
}

function setKindMode(mode) {
  state.kindMode = mode;
  els.kindModeToggle.classList.toggle("active", mode === "toggle");
  els.kindModeOnly.classList.toggle("active", mode === "only");
  renderKindFilters(applyKindFilter);
}

function loadFromObject(obj, label) {
  parseData(obj);
  buildTree();
  setExpandLevel("files");
  rebuildOutgoing();
  els.status.textContent = `${label} · ${obj.variables.length} entities · ${obj.cells.length} deps`;
  els.levelFiles.classList.add("active");
  els.levelFolders.classList.remove("active");
  els.levelSymbols.classList.remove("active");
  state.sortMode = "given";
  els.sortGiven.classList.add("active");
  els.sortName.classList.remove("active");
  els.sortLowerTri.classList.remove("active");
  updateZoomLabel();
  refresh();
}

async function loadDefault() {
  els.status.textContent = "Loading ../dep-result.json …";
  try {
    const res = await fetch("../dep-result.json", { cache: "no-store" });
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
    loadFromObject(await res.json(), "dep-result.json");
  } catch (err) {
    els.status.textContent =
      `Could not fetch default (${err.message}). Use Open or serve with python3 -m http.server`;
  }
}

function bindEvents() {
  const rowCanvas = getRowCanvas();

  rowCanvas.addEventListener("click", (ev) => {
    const r = Math.floor((ev.clientY - rowCanvas.getBoundingClientRect().top) / cellSize());
    if (r < 0 || r >= state.visible.length) return;
    selectEntity(state.visible[r].node.id);
    refresh();
  });

  els.canvas.addEventListener("mousemove", (ev) => {
    const hit = hitTest(ev);
    state.hover = hit;
    renderMatrix();
    if (hit) showCellHover(hit, ev.clientX, ev.clientY);
    else hideHoverCard();
  });

  els.canvas.addEventListener("mouseleave", () => {
    state.hover = null;
    hideHoverCard();
    renderMatrix();
  });

  els.canvas.addEventListener("click", (ev) => {
    const hit = hitTest(ev);
    if (!hit) return;
    selectCell(hit.src.id, hit.dest.id);
    refresh();
  });

  els.viewport.addEventListener("scroll", syncHeaderScroll);

  els.fileInput.addEventListener("change", async () => {
    const file = els.fileInput.files?.[0];
    if (!file) return;
    try {
      loadFromObject(JSON.parse(await file.text()), file.name);
    } catch (err) {
      els.status.textContent = `Failed to parse: ${err.message}`;
    }
  });

  els.loadDefault.addEventListener("click", loadDefault);
  els.search.addEventListener("input", () => {
    state.search = els.search.value;
    refresh(false);
  });
  els.sortGiven.addEventListener("click", () => setSort("given"));
  els.sortName.addEventListener("click", () => setSort("name"));
  els.sortLowerTri.addEventListener("click", () => setSort("lower-tri"));
  els.levelFolders.addEventListener("click", () => setLevel("folders"));
  els.levelFiles.addEventListener("click", () => setLevel("files"));
  els.levelSymbols.addEventListener("click", () => setLevel("symbols"));
  els.zoomIn.addEventListener("click", () => setZoom(state.zoom + ZOOM_STEP));
  els.zoomOut.addEventListener("click", () => setZoom(state.zoom - ZOOM_STEP));
  els.zoomReset.addEventListener("click", () => setZoom(1));
  els.kindModeToggle.addEventListener("click", () => setKindMode("toggle"));
  els.kindModeOnly.addEventListener("click", () => setKindMode("only"));
  els.kindReset.addEventListener("click", () => {
    state.enabledKinds = new Set(state.kinds);
    setKindMode("toggle");
    applyKindFilter();
  });

  window.addEventListener("keydown", (ev) => {
    if (ev.target.matches("input, textarea")) return;
    if (ev.key === "=" || ev.key === "+") setZoom(state.zoom + ZOOM_STEP);
    else if (ev.key === "-") setZoom(state.zoom - ZOOM_STEP);
    else if (ev.key === "0" && (ev.metaKey || ev.ctrlKey)) setZoom(1);
  });

  els.viewport.addEventListener(
    "wheel",
    (ev) => {
      if (!(ev.ctrlKey || ev.metaKey)) return;
      ev.preventDefault();
      setZoom(state.zoom + (ev.deltaY < 0 ? ZOOM_STEP : -ZOOM_STEP));
    },
    { passive: false }
  );

  window.addEventListener("resize", () => {
    if (state.data) refresh(false);
  });

  let dragging = false;
  let startX = 0;
  let startW = 0;
  els.labelResize.addEventListener("mousedown", (ev) => {
    ev.preventDefault();
    dragging = true;
    startX = ev.clientX;
    startW = state.labelW;
    els.labelResize.classList.add("dragging");
    document.body.classList.add("resizing-labels");
  });
  window.addEventListener("mousemove", (ev) => {
    if (!dragging) return;
    applyLabelWidth(startW + (ev.clientX - startX));
    renderRowHeader();
  });
  window.addEventListener("mouseup", () => {
    if (!dragging) return;
    dragging = false;
    els.labelResize.classList.remove("dragging");
    document.body.classList.remove("resizing-labels");
    try {
      localStorage.setItem("dsm-label-w", String(state.labelW));
    } catch (_) {}
    refresh(false);
  });
}

initView(els, () => refresh());
bindEvents();

try {
  applyLabelWidth(Number(localStorage.getItem("dsm-label-w")) || state.labelW);
} catch (_) {
  applyLabelWidth(state.labelW);
}

updateZoomLabel();
if (location.protocol === "http:" || location.protocol === "https:") {
  loadDefault();
}
