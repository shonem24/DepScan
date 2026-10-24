/** Parse JSON, build hierarchy, expand/visibility, and aggregate deps. */

import { state } from "./state.js";

function parseVariable(full, order) {
  const kindMatch = full.match(/\(([^)]+)\)\s*$/);
  const kind = kindMatch ? kindMatch[1] : "Unknown";
  const path = kindMatch ? full.slice(0, kindMatch.index).replace(/\s+$/, "") : full;
  const fileMatch = path.match(/^((?:[^/]+\/)*[^/]+\.[A-Za-z0-9_+-]+)/);
  const filePath = fileMatch ? fileMatch[1] : path.split("/").slice(0, 1).join("/");
  const rest = path.slice(filePath.length).replace(/^\//, "");
  const relParts = rest ? rest.split("/") : [];
  const name =
    relParts.length === 0 ? filePath.split("/").pop() : relParts[relParts.length - 1];
  return { full, filePath, relParts, kind, name, order };
}

function ensureChild(parent, id, patch) {
  let node = state.nodesById.get(id);
  if (!node) {
    node = {
      id,
      name: patch.name,
      path: patch.path || patch.name,
      kind: patch.kind,
      order: patch.order,
      entityIndices: [],
      children: [],
      parentId: parent ? parent.id : null,
    };
    state.nodesById.set(id, node);
    if (parent) parent.children.push(node);
    else state.roots.push(node);
  } else {
    node.order = Math.min(node.order, patch.order);
  }
  return node;
}

function rollEntityIndices(node) {
  for (const c of node.children) {
    rollEntityIndices(c);
    for (const idx of c.entityIndices) {
      if (!node.entityIndices.includes(idx)) node.entityIndices.push(idx);
    }
  }
  node.entityIndices.sort((a, b) => a - b);
}

export function parseData(raw) {
  if (!raw || !Array.isArray(raw.variables) || !Array.isArray(raw.cells)) {
    throw new Error("Invalid dep-result.json: expected { variables, cells }");
  }
  const kinds = new Set();
  for (const cell of raw.cells) {
    for (const k of Object.keys(cell.values || {})) kinds.add(k);
  }
  state.data = raw;
  state.kinds = [...kinds].sort();
  state.enabledKinds = new Set(state.kinds);
  state.selectedRowId = null;
  state.selectedColId = null;
  state.hover = null;
}

/**
 * Reorder nodes so dependencies prefer the lower triangle:
 * row (src) depends on col (dest) ⇒ dest should appear above/before src.
 * Cycles are broken by always emitting the remaining node with the fewest
 * unmet "must come after" constraints (then stable by original order).
 */
function lowerTriOrder(nodes) {
  const n = nodes.length;
  if (n <= 1) return nodes.slice();

  const adj = Array.from({ length: n }, () => []);
  const indeg = Array(n).fill(0);

  for (let src = 0; src < n; src++) {
    for (let dest = 0; dest < n; dest++) {
      if (src === dest) continue;
      if (aggregate(nodes[src], nodes[dest]).count <= 0) continue;
      // dest before src
      adj[dest].push(src);
      indeg[src]++;
    }
  }

  const remaining = new Set(Array.from({ length: n }, (_, i) => i));
  const out = [];

  while (remaining.size) {
    let best = -1;
    let bestDeg = Infinity;
    let bestOrder = Infinity;
    for (const i of remaining) {
      const deg = indeg[i];
      const ord = nodes[i].order;
      if (deg < bestDeg || (deg === bestDeg && ord < bestOrder)) {
        best = i;
        bestDeg = deg;
        bestOrder = ord;
      }
    }
    remaining.delete(best);
    out.push(nodes[best]);
    for (const succ of adj[best]) {
      if (remaining.has(succ)) indeg[succ]--;
    }
  }

  return out;
}

export function sortTree() {
  function walk(nodes) {
    if (nodes.length > 1) {
      if (state.sortMode === "name") {
        nodes.sort((a, b) =>
          a.name.localeCompare(b.name, undefined, { sensitivity: "base" })
        );
      } else if (state.sortMode === "lower-tri") {
        const ordered = lowerTriOrder(nodes);
        nodes.length = 0;
        nodes.push(...ordered);
      } else {
        nodes.sort((a, b) => a.order - b.order);
      }
    }
    for (const n of nodes) walk(n.children);
  }
  walk(state.roots);
}

export function buildTree() {
  state.roots = [];
  state.nodesById = new Map();
  const meta = state.data.variables.map((v, i) => parseVariable(v, i));

  for (let i = 0; i < meta.length; i++) {
    const m = meta[i];
    const segs = m.filePath.split("/");
    let parent = null;
    let pathAcc = "";

    for (let s = 0; s < segs.length - 1; s++) {
      pathAcc = pathAcc ? `${pathAcc}/${segs[s]}` : segs[s];
      parent = ensureChild(parent, `folder:${pathAcc}`, {
        name: segs[s],
        path: pathAcc,
        kind: "folder",
        order: m.order,
      });
    }

    const fileNode = ensureChild(parent, `file:${m.filePath}`, {
      name: segs[segs.length - 1],
      path: m.filePath,
      kind: "file",
      order: m.order,
    });

    if (m.kind === "File" && (m.relParts.length === 0 || m.relParts[0] === "self")) {
      fileNode.entityIndices.push(i);
      continue;
    }

    let entParent = fileNode;
    if (
      m.relParts.length >= 2 &&
      !["functions", "methods", "fields", "constructors"].includes(m.relParts[0])
    ) {
      const typeName = m.relParts[0];
      // kind "type" (not "folder") so Files expand level stays at .py/.ts files
      entParent = ensureChild(fileNode, `type:${m.filePath}/${typeName}`, {
        name: typeName,
        path: `${m.filePath}/${typeName}`,
        kind: "type",
        order: m.order,
      });
      if (m.relParts[1] === "self") {
        entParent.entityIndices.push(i);
        continue;
      }
    }

    ensureChild(entParent, `ent:${i}`, {
      name: `${m.name} (${m.kind})`,
      path: m.full,
      kind: "entity",
      order: m.order,
    }).entityIndices.push(i);
  }

  for (const r of state.roots) rollEntityIndices(r);
  sortTree();
}

/** @param {"folders"|"files"|"symbols"} level */
export function setExpandLevel(level) {
  state.expanded.clear();
  function walk(node) {
    if (level === "folders") {
      if (node.kind === "folder" && node.children.some((c) => c.kind === "folder")) {
        state.expanded.add(node.id);
      }
    } else if (level === "files") {
      // Path folders only — not class/type containers
      if (node.kind === "folder") state.expanded.add(node.id);
    } else if (level === "symbols") {
      if (node.kind === "folder" || node.kind === "file" || node.kind === "type") {
        state.expanded.add(node.id);
      }
    }
    for (const c of node.children) walk(c);
  }
  for (const r of state.roots) {
    // Don't force-expand file roots — otherwise "Files" drills into classes/functions
    if (r.kind === "folder" || level === "symbols") state.expanded.add(r.id);
    walk(r);
  }
}

export function toggleExpand(nodeId) {
  if (state.expanded.has(nodeId)) state.expanded.delete(nodeId);
  else state.expanded.add(nodeId);
}

export function collectVisible() {
  const out = [];
  const q = state.search.trim().toLowerCase();

  function matches(node) {
    if (!q) return true;
    if (node.name.toLowerCase().includes(q)) return true;
    return node.children.some(matches);
  }

  function walk(node, depth) {
    if (q && !matches(node)) return;
    const open = state.expanded.has(node.id) && node.children.length > 0;
    if (open) {
      for (const c of node.children) walk(c, depth + 1);
    } else {
      out.push({ node, depth });
    }
  }

  for (const r of state.roots) walk(r, 0);
  state.visible = out;
  state.aggCache.clear();
}

export function rebuildOutgoing() {
  const n = state.data.variables.length;
  state.outgoing = Array.from({ length: n }, () => []);
  for (const cell of state.data.cells) {
    const byKind = {};
    let weight = 0;
    for (const [k, v] of Object.entries(cell.values || {})) {
      if (!state.enabledKinds.has(k)) continue;
      const num = Number(v) || 0;
      byKind[k] = num;
      weight += num;
    }
    if (!weight) continue;
    state.outgoing[cell.src].push({ dest: cell.dest, byKind, weight });
  }
  state.aggCache.clear();
}

export function aggregate(srcNode, destNode) {
  const key = `${srcNode.id}|${destNode.id}`;
  if (state.aggCache.has(key)) return state.aggCache.get(key);

  const byKind = {};
  let count = 0;
  const destSet = new Set(destNode.entityIndices);

  for (const s of srcNode.entityIndices) {
    for (const edge of state.outgoing[s] || []) {
      if (!destSet.has(edge.dest) || edge.dest === s) continue;
      count += edge.weight;
      for (const [k, v] of Object.entries(edge.byKind)) {
        byKind[k] = (byKind[k] || 0) + v;
      }
    }
  }

  const result = { count, byKind };
  state.aggCache.set(key, result);
  return result;
}
