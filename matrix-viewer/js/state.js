/** Shared viewer state and sizing helpers. */

export const BASE_CELL = 26;
export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 2.5;
export const ZOOM_STEP = 0.15;

export const state = {
  data: null,
  kinds: [],
  enabledKinds: new Set(),
  roots: [],
  nodesById: new Map(),
  expanded: new Set(),
  sortMode: "given",
  kindMode: "toggle",
  search: "",
  visible: [],
  selectedRowId: null,
  selectedColId: null,
  hover: null,
  zoom: 1,
  labelW: 300,
  aggCache: new Map(),
  outgoing: [],
  dpr: Math.max(1, window.devicePixelRatio || 1),
};

export function cellSize() {
  return Math.max(12, Math.round(BASE_CELL * state.zoom));
}

export function headerH() {
  return Math.max(12, Math.round(cellSize() / 2));
}

export function selectEntity(nodeId) {
  state.selectedRowId = nodeId;
  state.selectedColId = nodeId;
}

export function selectCell(rowId, colId) {
  state.selectedRowId = rowId;
  state.selectedColId = colId;
}
