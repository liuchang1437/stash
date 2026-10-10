// Where the popover's panels go.
//
// Panels never push each other around: the card stays on the caret, the
// preview sits beside it, and the action menu takes the first free spot
// around them. Boxes are in CSS px relative to the card's top-left corner.
//
// The window is pinned to the caret by its top edge when opening down and by
// its bottom edge when opening up (placement.rs), and horizontally by
// `origin.x` (reported as `cardX`). So as long as nothing starts above the
// card when opening down, or ends below it when opening up, growing the
// window never moves anything already on screen.

import type { Space } from "./api";

export const MARGIN = 20;
export const GAP = 8;
export const MENU_W = 220;
/** Main menu + gap + submenu, reserved so a submenu always has room. */
export const MENU_FULL_W = MENU_W * 2 + 6;

export type Side = "left" | "right";
export type MenuSlot = { x: number; y: number; leftward: boolean };

export type Geometry = {
  /** Card position inside the window. */
  origin: { x: number; y: number };
  width: number;
  height: number;
  peek: { x: number; y: number } | null;
  menu: MenuSlot | null;
};

export type PanelInput = {
  space: Space;
  /** Opening upwards. */
  flip: boolean;
  cardHeight: number;
  /** Height of the shown preview, or null when it is closed. */
  peekHeight: number | null;
  /** Height of the open main menu, or null when it is closed. */
  menuHeight: number | null;
  /** How far an open submenu sticks out past the main menu. */
  subOverflow: number;
  /** List card and preview widths (`config.cardWidth` / `peekWidth`). */
  cardWidth: number;
  peekWidth: number;
};

type Box = { x: number; y: number; w: number; h: number };

/** Side panels go where there is room, keeping the card on the caret. */
export function sideFor(width: number, space: Space, cardWidth: number): Side {
  if (space.right >= cardWidth + GAP + width + MARGIN) return "right";
  return space.left > space.right - cardWidth ? "left" : "right";
}

export function placePanels(input: PanelInput): Geometry {
  const { space, flip, subOverflow, cardWidth, peekWidth } = input;
  const H = input.cardHeight;
  const room = {
    right: space.right - MARGIN,
    left: space.left - MARGIN,
    vertical: (flip ? space.above : space.below) - MARGIN - 4,
  };
  const alignY = (h: number) => (flip ? H - h : 0);
  const besideX = (side: Side, w: number) => (side === "right" ? cardWidth + GAP : -GAP - w);
  const fitsVertically = (y: number, h: number) =>
    flip ? y >= H - room.vertical && y + h <= H : y >= 0 && y + h <= room.vertical;
  // The main menu sits at x; its submenu opens away from the card.
  const menuExtent = (s: MenuSlot) => ({ x: s.leftward ? s.x - MENU_W - 6 : s.x, w: MENU_FULL_W });
  const fitsHorizontally = (s: MenuSlot) => {
    const e = menuExtent(s);
    return e.x >= -room.left && e.x + e.w <= room.right;
  };

  const boxes: Box[] = [{ x: 0, y: 0, w: cardWidth, h: H }];

  let peek: Geometry["peek"] = null;
  const peekSide = sideFor(peekWidth, space, cardWidth);
  if (input.peekHeight !== null) {
    peek = { x: besideX(peekSide, peekWidth), y: alignY(input.peekHeight) };
    boxes.push({ ...peek, w: peekWidth, h: input.peekHeight });
  }

  let menu: MenuSlot | null = null;
  if (input.menuHeight !== null) {
    const h = input.menuHeight;
    const candidates: MenuSlot[] = [];
    if (peek && input.peekHeight !== null) {
      const right = peekSide === "right";
      // 1. The preview's column, in the free space past the preview.
      candidates.push({
        x: right ? peek.x : peek.x + peekWidth - MENU_W,
        y: flip ? peek.y - GAP - h : peek.y + input.peekHeight + GAP,
        leftward: !right,
      });
      // 2. Beyond the preview.
      candidates.push({ x: right ? peek.x + peekWidth + GAP : peek.x - GAP - MENU_W, y: alignY(h), leftward: !right });
      // 3. The card's other side.
      candidates.push({ x: besideX(right ? "left" : "right", MENU_W), y: alignY(h), leftward: right });
    } else {
      const side = sideFor(MENU_FULL_W, space, cardWidth);
      candidates.push({ x: besideX(side, MENU_W), y: alignY(h), leftward: side === "left" });
    }
    // 4. No room anywhere: on top of the preview, at its inner edge.
    menu = candidates.find((c) => fitsVertically(c.y, h) && fitsHorizontally(c)) ?? { ...candidates[0], y: alignY(h) };
    const e = menuExtent(menu);
    boxes.push({ x: e.x, y: flip ? menu.y - subOverflow : menu.y, w: e.w, h: h + subOverflow });
  }

  const minX = Math.min(...boxes.map((b) => b.x));
  const maxX = Math.max(...boxes.map((b) => b.x + b.w));
  const minY = Math.min(...boxes.map((b) => b.y));
  const maxY = Math.max(...boxes.map((b) => b.y + b.h));
  return {
    origin: { x: MARGIN - minX, y: MARGIN - minY },
    width: maxX - minX + 2 * MARGIN,
    height: maxY - minY + 2 * MARGIN,
    peek,
    menu,
  };
}
