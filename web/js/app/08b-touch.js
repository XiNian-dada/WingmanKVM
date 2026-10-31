
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
    // Native 1:1 mode with the pan tool active keeps its own drag handling.
    if (consoleBox.classList.contains('mode-native') && isPanActive) return false;
    event.preventDefault();
    event.stopImmediatePropagation();
    return true;
  }

  viewport.addEventListener(
    'pointerdown',
    (event) => {
      if (!claimTouch(event)) return;
      if (!inputEnabled()) {
        // Same as a mouse: the first tap takes control instead of clicking.
        setRemote(true);
        viewport.focus({ preventScroll: true });
        touchGesture = 'consumed';
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
      } else if (touches.size === 2) {
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
      if (event.pointerType !== 'touch' || !touches.has(event.pointerId)) return;
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
      if (touches.size !== 1 || touchGesture === 'longpress') return;
      if (
        touchGesture === 'pending' &&
        Math.hypot(touch.x - touch.startX, touch.y - touch.startY) < TOUCH_TAP_SLOP_PX
      )
        return;
      cancelLongPress();
      touchGesture = 'drag';
      if (absoluteMode()) {
        absolutePending = mapAbsolute(touch.x, touch.y);
      } else {
        mouseX += Math.round((touch.x - touchLastX) * TOUCH_TRACKPAD_GAIN);
        mouseY += Math.round((touch.y - touchLastY) * TOUCH_TRACKPAD_GAIN);
      }
      touchLastX = touch.x;
      touchLastY = touch.y;
      scheduleMouse();
    },
    true,
  );

  function endTouch(event) {
    if (event.pointerType !== 'touch') return;
    if (touchGesture === 'consumed') {
      event.preventDefault();
      event.stopImmediatePropagation();
      touchGesture = 'none';
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
    }
  }
  viewport.addEventListener('pointerup', endTouch, true);
  viewport.addEventListener('pointercancel', endTouch, true);
