/** Canvas matrix + tree / filters / hover UI. */

import { state, cellSize, headerH, selectEntity } from "./state.js";
import { aggregate, toggleExpand } from "./data.js";

/** @type {Record<string, HTMLElement>} */
let els = null;
let colCanvas = null;
let rowCanvas = null;
/** @type {null | (() => void)} */
let onRefresh = null;

export function initView(elements, refreshFn) {
  els = elements;
  onRefresh = refreshFn;
  colCanvas = document.createElement("canvas");
  rowCanvas = document.createElement("canvas");
  els.colHeader.appendChild(colCanvas);
  els.rowHeader.appendChild(rowCanvas);
}

export function getRowCanvas() {
  return rowCanvas;
}

export function syncHeaderScroll() {
  colCanvas.style.transform = `translateX(${-els.viewport.scrollLeft}px)`;
  rowCanvas.style.transform = `translateY(${-els.viewport.scrollTop}px)`;
}

export function renderKindFilters(onKindChange) {
  els.kindFilters.innerHTML = "";
  const solo =
    state.kindMode === "only" && state.enabledKinds.size === 1
      ? [...state.enabledKinds][0]
      : null;

  for (const kind of state.kinds) {
    const on = state.enabledKinds.has(kind);
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "kind-chip" + (on ? "" : " off") + (solo === kind ? " solo" : "");
    chip.textContent = kind;
    chip.title =
      state.kindMode === "only"
        ? `Show only ${kind}`
        : `Turn ${kind} ${on ? "off" : "on"}`;
    chip.addEventListener("click", () => {
      if (state.kindMode === "only") {
        if (solo === kind) state.enabledKinds = new Set(state.kinds);
        else state.enabledKinds = new Set([kind]);
      } else if (on) {
        if (state.enabledKinds.size === 1) return;
        state.enabledKinds.delete(kind);
      } else {
        state.enabledKinds.add(kind);
      }
      onKindChange();
    });
    els.kindFilters.appendChild(chip);
  }
}

export function renderTree() {
  const frag = document.createDocumentFragment();

  function walk(node, depth) {
    const row = document.createElement("div");
    row.className =
      "tree-item" +
      (state.selectedRowId === node.id || state.selectedColId === node.id
        ? " selected"
        : "");
    row.style.paddingLeft = `${6 + depth * 14}px`;

    const twist = document.createElement("span");
    twist.className = "twist" + (node.children.length ? "" : " empty");
    const open = state.expanded.has(node.id);
    twist.textContent = node.children.length ? (open ? "▼" : "▶") : "";
    twist.addEventListener("click", (e) => {
      e.stopPropagation();
      if (!node.children.length) return;
      toggleExpand(node.id);
      onRefresh();
    });

    const name = document.createElement("span");
    name.className = "name";
    name.textContent = node.name;

    row.append(twist, name);
    row.addEventListener("click", () => {
      selectEntity(node.id);
      let p = node.parentId;
      while (p) {
        state.expanded.add(p);
        p = state.nodesById.get(p)?.parentId;
      }
      onRefresh();
    });
    row.addEventListener("dblclick", () => {
      if (node.children.length) {
        toggleExpand(node.id);
        onRefresh();
      }
    });
    frag.appendChild(row);
    if (open) for (const c of node.children) walk(c, depth + 1);
  }

  for (const r of state.roots) walk(r, 0);
  els.tree.innerHTML = "";
  els.tree.appendChild(frag);
}

export function renderRowHeader() {
  const dpr = state.dpr;
  const CELL = cellSize();
  const n = state.visible.length;
  const W = els.rowHeader.clientWidth || 280;
  const H = n * CELL;
  const fontPx = Math.max(10, Math.min(13, Math.round(11 * state.zoom)));
  const padRight = 6;
  rowCanvas.width = Math.max(1, Math.floor(W * dpr));
  rowCanvas.height = Math.max(1, Math.floor(H * dpr));
  rowCanvas.style.width = `${W}px`;
  rowCanvas.style.height = `${H}px`;

  const ctx = rowCanvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  ctx.textBaseline = "middle";
  ctx.textAlign = "right";

  for (let i = 0; i < n; i++) {
    const { node, depth } = state.visible[i];
    const y = i * CELL;
    ctx.fillStyle = state.selectedRowId === node.id ? "#fff59d" : "#ffffff";
    ctx.fillRect(0, y, W, CELL);

    const maxW = Math.max(40, W - padRight - depth * 10 - 8);
    ctx.fillStyle = "#000";
    ctx.font = `${fontPx}px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`;
    const labelBase =
      node.kind === "file" || node.kind === "folder" ? node.path || node.name : node.name;
    let text = `${labelBase} - ${i + 1}`;
    if (ctx.measureText(text).width > maxW) {
      while (text.length > 4 && ctx.measureText(`…${text}`).width > maxW) {
        text = text.slice(1);
      }
      text = `…${text}`;
    }
    ctx.fillText(text, W - padRight, y + CELL / 2);
  }

  ctx.strokeStyle = "#e0e0e0";
  ctx.beginPath();
  for (let i = 1; i < n; i++) {
    const y = i * CELL + 0.5;
    ctx.moveTo(0, y);
    ctx.lineTo(W, y);
  }
  ctx.stroke();
}

export function renderColHeader() {
  const dpr = state.dpr;
  const CELL = cellSize();
  const n = state.visible.length;
  const W = n * CELL;
  const H = headerH();
  const fontPx = Math.max(8, Math.min(11, Math.round(H * 0.72)));
  colCanvas.width = Math.max(1, Math.floor(W * dpr));
  colCanvas.height = Math.max(1, Math.floor(H * dpr));
  colCanvas.style.width = `${W}px`;
  colCanvas.style.height = `${H}px`;

  const ctx = colCanvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  ctx.font = `${fontPx}px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";

  for (let i = 0; i < n; i++) {
    const node = state.visible[i].node;
    const x = i * CELL;
    ctx.fillStyle = state.selectedColId === node.id ? "#fff59d" : "#f7f7f7";
    ctx.fillRect(x, 0, CELL, H);
    ctx.fillStyle = "#222";
    ctx.fillText(String(i + 1), x + CELL / 2, H / 2);
  }

  ctx.strokeStyle = "#e0e0e0";
  ctx.beginPath();
  for (let i = 0; i <= n; i++) {
    const x = i * CELL + 0.5;
    ctx.moveTo(x, 0);
    ctx.lineTo(x, H);
  }
  ctx.moveTo(0, H - 0.5);
  ctx.lineTo(W, H - 0.5);
  ctx.stroke();
}

export function renderMatrix() {
  const dpr = state.dpr;
  const CELL = cellSize();
  const n = state.visible.length;
  const W = n * CELL;
  const H = n * CELL;
  const fontPx = Math.max(9, Math.min(13, Math.round(11 * state.zoom)));
  const canvas = els.canvas;
  canvas.width = Math.max(1, Math.floor(W * dpr));
  canvas.height = Math.max(1, Math.floor(H * dpr));
  canvas.style.width = `${W}px`;
  canvas.style.height = `${H}px`;

  const ctx = canvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, W, H);

  const selRow = state.selectedRowId
    ? state.visible.findIndex((v) => v.node.id === state.selectedRowId)
    : -1;
  const selCol = state.selectedColId
    ? state.visible.findIndex((v) => v.node.id === state.selectedColId)
    : -1;

  if (selRow >= 0) {
    ctx.fillStyle = "#fff59d";
    ctx.fillRect(0, selRow * CELL, W, CELL);
  }
  if (selCol >= 0) {
    ctx.fillStyle = "#fff59d";
    ctx.fillRect(selCol * CELL, 0, CELL, H);
  }

  ctx.textAlign = "center";
  ctx.textBaseline = "middle";

  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const x = c * CELL;
      const y = r * CELL;
      if (r === c) {
        if (selRow !== r && selCol !== c) {
          ctx.fillStyle = "#efefef";
          ctx.fillRect(x, y, CELL, CELL);
        }
        ctx.fillStyle = "#222";
        ctx.font = `bold ${fontPx}px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`;
        const label = String(r + 1);
        ctx.fillText(label, x + CELL / 2, y + CELL / 2);
        const tw = ctx.measureText(label).width;
        ctx.beginPath();
        ctx.moveTo(x + CELL / 2 - tw / 2, y + CELL / 2 + fontPx * 0.45);
        ctx.lineTo(x + CELL / 2 + tw / 2, y + CELL / 2 + fontPx * 0.45);
        ctx.strokeStyle = "#222";
        ctx.stroke();
        continue;
      }

      const agg = aggregate(state.visible[r].node, state.visible[c].node);
      if (!agg.count) continue;
      ctx.fillStyle = "#222";
      ctx.font = `${fontPx}px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`;
      ctx.fillText(String(agg.count), x + CELL / 2, y + CELL / 2);
    }
  }

  ctx.strokeStyle = "#e0e0e0";
  ctx.beginPath();
  for (let i = 0; i <= n; i++) {
    const p = i * CELL + 0.5;
    ctx.moveTo(p, 0);
    ctx.lineTo(p, H);
    ctx.moveTo(0, p);
    ctx.lineTo(W, p);
  }
  ctx.stroke();

  // Darker border on diagonal cells (drawn after grid so it sits on top)
  ctx.strokeStyle = "#888888";
  ctx.lineWidth = 1.25;
  for (let i = 0; i < n; i++) {
    ctx.strokeRect(i * CELL + 0.5, i * CELL + 0.5, CELL - 1, CELL - 1);
  }
  ctx.lineWidth = 1;

  if (state.hover) {
    ctx.strokeStyle = "#4b6eaf";
    ctx.lineWidth = 1.5;
    ctx.strokeRect(state.hover.c * CELL + 1, state.hover.r * CELL + 1, CELL - 2, CELL - 2);
    ctx.lineWidth = 1;
  }
}

export function updateStats() {
  const n = state.visible.length;
  els.stats.textContent = `${n} × ${n} · ${state.data.cells.length} deps · ${Math.round(state.zoom * 100)}%`;
}

function nodeLabel(node) {
  if (node.kind === "file" || node.kind === "folder") return node.path || node.name;
  return node.name;
}

export function hideHoverCard() {
  els.hoverCard.hidden = true;
}

export function showCellHover(hit, clientX, clientY) {
  if (hit.r === hit.c) {
    hideHoverCard();
    return;
  }
  const agg = aggregate(hit.src, hit.dest);
  if (!agg.count) {
    hideHoverCard();
    return;
  }
  const kinds = Object.entries(agg.byKind)
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([k, v]) => `${k} (${v})`)
    .join("\n");
  els.hoverCard.innerHTML = `${nodeLabel(hit.src)} -> ${nodeLabel(hit.dest)} (${agg.count})<div class="kind-lines">${kinds}</div>`;

  const card = els.hoverCard;
  card.hidden = false;
  const pad = 12;
  const rect = card.getBoundingClientRect();
  let left = clientX + pad;
  let top = clientY + pad;
  if (left + rect.width > window.innerWidth - 8) left = clientX - rect.width - pad;
  if (top + rect.height > window.innerHeight - 8) top = clientY - rect.height - pad;
  card.style.left = `${Math.max(8, left)}px`;
  card.style.top = `${Math.max(8, top)}px`;
}

export function hitTest(ev) {
  const CELL = cellSize();
  const rect = els.canvas.getBoundingClientRect();
  const c = Math.floor((ev.clientX - rect.left) / CELL);
  const r = Math.floor((ev.clientY - rect.top) / CELL);
  const n = state.visible.length;
  if (r < 0 || c < 0 || r >= n || c >= n) return null;
  return { r, c, src: state.visible[r].node, dest: state.visible[c].node };
}
