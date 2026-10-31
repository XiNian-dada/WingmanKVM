  // Adaptive WebRTC bitrate (#82). Every second the quality sampler (07b)
  // hands the latest getStats() numbers to decideAdaptiveBitrate(), a pure
  // function, and the chosen target goes to POST /api/webrtc/bitrate. The
  // server clamps it to [256 kbps, configured bitrate] and restarts only the
  // FFmpeg encoder (no renegotiation), so changes are deliberately rare.
  const ADAPTIVE_POLICY = {
    // A one-second sample is "bad" when any congestion signal crosses its line:
    badLossPct: 5, //    ≥5 % of video packets lost: visible smearing/freezes
    badRttMs: 350, //    queues are building somewhere on the path
    badJitterMs: 200, // the jitter buffer holds >200 ms: delivery comes in bursts
    // RTT trend: well above the RTT seen while healthy also counts as bad,
    // which catches bufferbloat on links whose base RTT is already high.
    rttRiseFactor: 2,
    rttRiseMinMs: 100,
    severeLossPct: 15, // heavy loss: halve instead of stepping down
    // "Good" requires every known signal to be comfortably healthy. The gap
    // between the good and bad lines is the hysteresis band: samples inside it
    // reset both streaks and never change the bitrate.
    goodLossPct: 1,
    goodRttMs: 150,
    goodJitterMs: 80,
    badSamples: 3, //   ≈3 s of consecutive bad samples before lowering
    goodSamples: 10, // ≈10 s of consecutive good samples before raising
    decreaseFactor: 0.7,
    severeFactor: 0.5,
    increaseFactor: 1.25,
    // Under loss the target also never exceeds what actually arrived (+10 %).
    receivedHeadroom: 1.1,
    cooldownMs: 10000, //     min spacing of changes; the server enforces it too
    raiseAfterBadMs: 30000, // no raise within 30 s of the last bad sample
    stepKbps: 50,
    snapToMaxRatio: 0.95, //  a raise this close to the maximum goes all the way
  };
  function newAdaptiveState(maxKbps, minKbps = 256) {
    const max = Math.max(1, Math.round(Number(maxKbps) || 4000));
    return {
      currentKbps: max,
      maxKbps: max,
      minKbps: Math.min(max, Math.max(1, Math.round(Number(minKbps) || 256))),
      badStreak: 0,
      goodStreak: 0,
      lastChangeAt: null,
      lastBadAt: null,
      rttBaselineMs: null,
    };
  }
  const known = (value) => typeof value === 'number' && Number.isFinite(value);
  // Pure: returns { state, targetKbps, verdict } and never mutates its inputs.
  // targetKbps is null when the bitrate should stay; otherwise the returned
  // state already assumes the change (the caller reverts on a failed request).
  function decideAdaptiveBitrate(previous, sample, now, policy = ADAPTIVE_POLICY) {
    const state = { ...previous },
      loss = known(sample?.packetLossPct) ? sample.packetLossPct : null,
      rtt = known(sample?.rttMs) ? sample.rttMs : null,
      jitter = known(sample?.jitterBufferMs) ? sample.jitterBufferMs : null,
      received = known(sample?.receivedKbps) ? sample.receivedKbps : null;
    if (loss === null && rtt === null && jitter === null) return { state, targetKbps: null, verdict: 'unknown' };
    const base = state.rttBaselineMs,
      rttRising = rtt !== null && base !== null && rtt >= base * policy.rttRiseFactor && rtt - base >= policy.rttRiseMinMs,
      bad =
        (loss !== null && loss >= policy.badLossPct) ||
        (rtt !== null && rtt >= policy.badRttMs) ||
        (jitter !== null && jitter >= policy.badJitterMs) ||
        rttRising,
      good =
        !bad &&
        (loss === null || loss < policy.goodLossPct) &&
        (rtt === null || rtt < policy.goodRttMs) &&
        (jitter === null || jitter < policy.goodJitterMs);
    const verdict = bad ? 'bad' : good ? 'good' : 'neutral';
    // Learn the healthy RTT only from good samples (slow EWMA).
    if (good && rtt !== null) state.rttBaselineMs = base === null ? rtt : base * 0.9 + rtt * 0.1;
    if (bad) {
      state.badStreak = Math.min(state.badStreak + 1, policy.badSamples);
      state.goodStreak = 0;
      state.lastBadAt = now;
    } else if (good) {
      state.goodStreak = Math.min(state.goodStreak + 1, policy.goodSamples);
      state.badStreak = 0;
    } else {
      state.badStreak = 0;
      state.goodStreak = 0;
    }
    const cooled = state.lastChangeAt === null || now - state.lastChangeAt >= policy.cooldownMs,
      round = (kbps) =>
        Math.min(state.maxKbps, Math.max(state.minKbps, Math.round(kbps / policy.stepKbps) * policy.stepKbps));
    let target = null;
    if (bad && state.badStreak >= policy.badSamples && cooled && state.currentKbps > state.minKbps) {
      const severe = loss !== null && loss >= policy.severeLossPct;
      let next = state.currentKbps * (severe ? policy.severeFactor : policy.decreaseFactor);
      if (loss !== null && loss >= policy.badLossPct && received !== null && received > 0)
        next = Math.min(next, received * policy.receivedHeadroom);
      target = round(next);
      if (target >= state.currentKbps) target = Math.max(state.minKbps, state.currentKbps - policy.stepKbps);
    } else if (
      good &&
      state.goodStreak >= policy.goodSamples &&
      cooled &&
      state.currentKbps < state.maxKbps &&
      (state.lastBadAt === null || now - state.lastBadAt >= policy.raiseAfterBadMs)
    ) {
      target = round(state.currentKbps * policy.increaseFactor);
      if (target >= state.maxKbps * policy.snapToMaxRatio) target = state.maxKbps;
      if (target <= state.currentKbps) target = null;
    }
    if (target === null || target === state.currentKbps) return { state, targetKbps: null, verdict };
    state.currentKbps = target;
    state.lastChangeAt = now;
    state.badStreak = 0;
    state.goodStreak = 0;
    return { state, targetKbps: target, verdict };
  }

  const adaptive = {
    enabled: getSetting('adaptive_bitrate', true) !== false,
    sessionId: null,
    state: null,
    pending: false,
    restoreTimer: 0,
  };
  function adaptiveBitrateWantsStats() {
    return adaptive.enabled && activeVideoTransport === 'webrtc_h264';
  }
  // A new WebRTC session always starts at the configured bitrate.
  function resetAdaptiveBitrate(sessionId = null, maxKbps = null, minKbps = null) {
    clearTimeout(adaptive.restoreTimer);
    adaptive.sessionId = sessionId;
    adaptive.pending = false;
    adaptive.state = sessionId
      ? newAdaptiveState(maxKbps ?? bootstrap?.webrtc?.bitrate_kbps, minKbps ?? 256)
      : null;
  }
  async function sendAdaptiveBitrate(kbps, previousKbps) {
    const sessionId = adaptive.sessionId;
    adaptive.pending = true;
    try {
      const result = await request('/api/webrtc/bitrate', {
        method: 'POST',
        body: JSON.stringify({ session_id: sessionId, kbps }),
      });
      if (sessionId !== adaptive.sessionId || !adaptive.state) return true;
      Object.assign(adaptive.state, {
        currentKbps: Number(result?.kbps) || kbps,
        maxKbps: Number(result?.max_kbps) || adaptive.state.maxKbps,
        minKbps: Number(result?.min_kbps) || adaptive.state.minKbps,
      });
      return true;
    } catch (error) {
      if (sessionId !== adaptive.sessionId || !adaptive.state) return false;
      // Not applied: undo the optimistic value but keep lastChangeAt, which
      // backs off for a full cooldown before the next attempt.
      if (adaptive.state.currentKbps === kbps) adaptive.state.currentKbps = previousKbps;
      if (error?.status === 404 || error?.status === 401) adaptive.sessionId = null;
      return false;
    } finally {
      if (sessionId === adaptive.sessionId) adaptive.pending = false;
      renderQualityOverlay();
    }
  }
  // Called by sampleQuality() after each WebRTC stats sample.
  function adaptiveBitrateTick(now = performance.now()) {
    if (!adaptive.enabled || !adaptive.state || adaptive.pending || adaptive.sessionId !== webrtcSessionId) return;
    const q = connectionQuality,
      previousKbps = adaptive.state.currentKbps,
      { state, targetKbps } = decideAdaptiveBitrate(
        adaptive.state,
        { packetLossPct: q.packetLossPct, rttMs: q.rttMs, jitterBufferMs: q.jitterBufferMs, receivedKbps: q.bitrateKbps },
        now,
      );
    adaptive.state = state;
    if (targetKbps !== null) sendAdaptiveBitrate(targetKbps, previousKbps);
  }
  // Turning adaptation off hands the session back its configured bitrate.
  async function restoreConfiguredBitrate() {
    clearTimeout(adaptive.restoreTimer);
    const s = adaptive.state;
    if (adaptive.enabled || !s || !adaptive.sessionId || s.currentKbps >= s.maxKbps) return;
    if (adaptive.pending) {
      adaptive.restoreTimer = setTimeout(restoreConfiguredBitrate, 1000);
      return;
    }
    const previous = s.currentKbps;
    s.currentKbps = s.maxKbps;
    if (!(await sendAdaptiveBitrate(s.maxKbps, previous)) && adaptive.sessionId)
      adaptive.restoreTimer = setTimeout(restoreConfiguredBitrate, ADAPTIVE_POLICY.cooldownMs);
  }
  function adaptiveBitrateText() {
    if (!adaptive.enabled) return '已关闭';
    const s = adaptive.state;
    if (!s || !adaptive.sessionId) return '—';
    const text = `${qualityBitrateText(s.currentKbps)} / ${qualityBitrateText(s.maxKbps)}`;
    return s.currentKbps < s.maxKbps ? `↓ ${text}` : text;
  }
  function setAdaptiveBitrateEnabled(enabled) {
    adaptive.enabled = enabled;
    setSetting('adaptive_bitrate', enabled);
    if (!enabled) restoreConfiguredBitrate();
    else clearTimeout(adaptive.restoreTimer);
    syncQualityOverlay();
    renderQualityOverlay();
  }
  // The Playwright suite drives the pure decision function directly.
  window.__wingmanTest = Object.assign(window.__wingmanTest || {}, {
    decideAdaptiveBitrate,
    newAdaptiveState,
    adaptivePolicy: ADAPTIVE_POLICY,
    adaptiveBitrate: () => (adaptive.state ? { ...adaptive.state, enabled: adaptive.enabled } : { enabled: adaptive.enabled }),
  });
