import { shared } from './state.js';
import { consoleBox, viewport } from './00-core.js';
import { request } from './03-session.js';
import {
  absoluteMode, handleHidError, inputEnabled, mapAbsolute, scheduleMouse, setRemote,
} from './08-input.js';

// Touch gestures on the remote screen. Mouse and pen keep using the handlers
// in 08-input.js; touch pointers are intercepted here (capture phase) and
// turned into mouse actions that make sense on a phone or tablet:
//   tap = left click, long press = right click,
//   one-finger drag = move the pointer (follows the finger in absolute
//   mode, behaves like a trackpad in relative mode),
//   two-finger vertical drag = scroll wheel.
const TOUCH_TAP_SLOP_PX = 8;
const TOUCH_LONG_PRESS_MS = 550;
const TOUCH_SCROLL_STEP_PX = 30;
const TOUCH_TRACKPAD_GAIN = 1.5;
const touches = new Map();
// Fingers whose only job was to take control; their later events are ignored.
const takeoverTouches = new Set();
let touchGesture = 'none',
  touchLongPress = 0,
  touchScrollY = 0,
  touchLastX = 0,
  touchLastY = 0;

function touchClick(clientX, clientY, button) {
  if (absoluteMode()) {
    const point = mapAbsolute(clientX, clientY);
    if (!point) return;
    request('/api/mouse/absolute', {
      method: 'POST',
      body: JSON.stringify({ action: 'click', ...point, button }),
    }).catch(handleHidError);
  } else
    request('/api/mouse/click', { method: 'POST', body: JSON.stringify({ button }) }).catch(handleHidError);
}
function touchScroll(clientX, clientY, wheel) {
  if (absoluteMode()) {
    const point = mapAbsolute(clientX, clientY);
    if (!point) return;
    request('/api/mouse/absolute', {
      method: 'POST',
      body: JSON.stringify({ action: 'scroll', ...point, delta: wheel }),
    }).catch(handleHidError);
  } else request('/api/mouse/scroll', { method: 'POST', body: JSON.stringify({ wheel }) }).catch(handleHidError);
}
function touchMidpoint() {
  const points = [...touches.values()];
  return {
    x: points.reduce((sum, p) => sum + p.x, 0) / points.length,
    y: points.reduce((sum, p) => sum + p.y, 0) / points.length,
  };
}
function cancelLongPress() {
  clearTimeout(touchLongPress);
  touchLongPress = 0;
}
function claimTouch(event) {
  if (event.pointerType !== 'touch') return false;
  // Native 1:1 mode keeps its own drag-to-pan while the pan tool is active
  // or input is not being forwarded, exactly as before touch gestures.
  if (consoleBox.classList.contains('mode-native') && (shared.isPanActive || !inputEnabled())) return false;
  event.preventDefault();
  event.stopImmediatePropagation();
  return true;
}

function endTouch(event) {
  if (event.pointerType !== 'touch') return;
  if (takeoverTouches.delete(event.pointerId)) {
    event.preventDefault();
    event.stopImmediatePropagation();
    return;
  }
  if (!touches.has(event.pointerId)) return;
  claimTouch(event);
  const touch = touches.get(event.pointerId);
  touches.delete(event.pointerId);
  if (event.type === 'pointerup' && touchGesture === 'pending' && touches.size === 0) {
    cancelLongPress();
    touchClick(touch.x, touch.y, 1);
  }
  if (touches.size === 0) {
    cancelLongPress();
    touchGesture = 'none';
  } else if (touchGesture === 'scroll') {
    // Lifting a finger ends the scroll; the remaining finger does nothing
    // until all fingers are up, so it cannot turn into a stray drag or tap.
    touchGesture = 'ended';
  }
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  viewport.addEventListener(
    'pointerdown',
    (event) => {
      if (!claimTouch(event)) return;
      if (!inputEnabled()) {
        // Same as a mouse: the first tap takes control instead of clicking.
        setRemote(true);
        viewport.focus({ preventScroll: true });
        takeoverTouches.add(event.pointerId);
        return;
      }
      try {
        viewport.setPointerCapture?.(event.pointerId);
      } catch (_) {}
      touches.set(event.pointerId, { x: event.clientX, y: event.clientY, startX: event.clientX, startY: event.clientY });
      if (touches.size === 1) {
        touchGesture = 'pending';
        touchLastX = event.clientX;
        touchLastY = event.clientY;
        touchLongPress = setTimeout(() => {
          touchLongPress = 0;
          if (touchGesture !== 'pending') return;
          touchGesture = 'longpress';
          touchClick(event.clientX, event.clientY, 2);
          navigator.vibrate?.(15);
        }, TOUCH_LONG_PRESS_MS);
      } else if (touchGesture === 'pending' || touchGesture === 'scroll') {
        // A second (or further) finger: scroll, re-basing on the new midpoint
        // so a changing finger count never turns into a burst of wheel steps.
        cancelLongPress();
        touchGesture = 'scroll';
        touchScrollY = touchMidpoint().y;
      }
    },
    true,
  );

  viewport.addEventListener(
    'pointermove',
    (event) => {
      if (event.pointerType !== 'touch') return;
      if (takeoverTouches.has(event.pointerId)) {
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      if (!touches.has(event.pointerId)) return;
      claimTouch(event);
      const touch = touches.get(event.pointerId);
      touch.x = event.clientX;
      touch.y = event.clientY;
      if (touchGesture === 'scroll') {
        const mid = touchMidpoint(),
          steps = Math.trunc((touchScrollY - mid.y) / TOUCH_SCROLL_STEP_PX);
        if (steps) {
          touchScrollY -= steps * TOUCH_SCROLL_STEP_PX;
          // Fingers moving up scroll the content down, as on a phone.
          touchScroll(mid.x, mid.y, Math.max(-127, Math.min(127, -steps)));
        }
        return;
      }
      if (touches.size !== 1 || touchGesture === 'longpress' || touchGesture === 'ended') return;
      if (
        touchGesture === 'pending' &&
        Math.hypot(touch.x - touch.startX, touch.y - touch.startY) < TOUCH_TAP_SLOP_PX
      )
        return;
      cancelLongPress();
      touchGesture = 'drag';
      if (absoluteMode()) {
        shared.absolutePending = mapAbsolute(touch.x, touch.y);
      } else {
        shared.mouseX += Math.round((touch.x - touchLastX) * TOUCH_TRACKPAD_GAIN);
        shared.mouseY += Math.round((touch.y - touchLastY) * TOUCH_TRACKPAD_GAIN);
      }
      touchLastX = touch.x;
      touchLastY = touch.y;
      scheduleMouse();
    },
    true,
  );
  viewport.addEventListener('pointerup', endTouch, true);
  viewport.addEventListener('pointercancel', endTouch, true);
}
