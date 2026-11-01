import { shared } from './state.js';
import { $ } from './00-core.js';
import { tr } from './01-i18n.js';
import { request, toast } from './03-session.js';
import { connectionQuality, qualityBitrateText } from './07b-quality.js';
import { adaptive } from './07c-adaptive-bitrate.js';
import { handleHidError, renderKeyboardLeds } from './08-input.js';
import { inputSocketReady } from './08a-input-socket.js';

// Latency tool (#94). Only the HID round trip is measured: the browser sends
// Caps Lock and times until it reads the target's LED output report back.
// That covers browser → server → USB gadget → target OS → LED report →
// server → browser polling; it says nothing about video latency, which
// cannot be measured without software on the target, so none is shown.
const LATENCY_SAMPLES = 10; // even: the toggles alone leave Caps Lock as found
const LATENCY_TIMEOUT_MS = 1500;
const LATENCY_GAP_MS = 150;
const LATENCY_POLL_GAP_MS = 5;
const LATENCY_RESTORE_SETTLE_MS = 300;
const LATENCY_MAX_MISSES = 2; // consecutive: this target is not reporting LEDs
const latencyWait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function readCapsLock() {
  const leds = await request('/api/hid/leds');
  renderKeyboardLeds(leds);
  return !!leds?.caps_lock;
}
async function tapCapsLock() {
  const res = await request('/api/key', {
    method: 'POST',
    body: JSON.stringify({ key: 'CapsLock', ctrl: false, shift: false, alt: false, meta: false, hold_ms: 25 }),
  });
  if (res?.keyboard_leds) renderKeyboardLeds(res.keyboard_leds);
  return res?.keyboard_leds ? !!res.keyboard_leds.caps_lock : null;
}
// One sample: ms from sending the key until the LED is seen flipped, or null.
// Polling runs alongside the request because /api/key only answers after the
// key is released, which would otherwise put the hold time in every sample.
async function latencySample(run, before) {
  const started = performance.now();
  let seen = null,
    tapFailed = null;
  const tap = tapCapsLock().then(
    (caps) => {
      if (caps === !before && seen === null) seen = performance.now() - started;
    },
    (err) => {
      tapFailed = err;
    },
  );
  while (seen === null && !tapFailed && !run.cancelled && performance.now() - started < LATENCY_TIMEOUT_MS) {
    const asked = performance.now(),
      caps = await readCapsLock();
    run.pollMs.push(performance.now() - asked);
    if (caps === !before && seen === null) seen = performance.now() - started;
    if (seen === null) await latencyWait(LATENCY_POLL_GAP_MS);
  }
  // Never start the next press while this one may still be held.
  await tap;
  if (tapFailed) throw tapFailed;
  return seen;
}
// Wait until a late LED report has landed; true once Caps Lock reads `wanted`.
async function waitForCapsLock(wanted) {
  const started = performance.now();
  while (performance.now() - started < LATENCY_TIMEOUT_MS) {
    if ((await readCapsLock()) === wanted) return true;
    await latencyWait(LATENCY_GAP_MS);
  }
  return false;
}
function latencyPercentile(sorted, p) {
  return sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1))];
}
function latencyStats(samples) {
  const sorted = samples.filter((v) => v !== null).sort((a, b) => a - b),
    n = sorted.length,
    median = n === 0 ? null : n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
  return {
    count: n,
    attempts: samples.length,
    min: n ? sorted[0] : null,
    median,
    p95: n ? latencyPercentile(sorted, 95) : null,
  };
}
function latencyVideoText() {
  if (shared.activeVideoTransport === 'webrtc_h264') return 'H.264 (WebRTC)';
  if (shared.activeVideoTransport === 'mjpeg') return 'MJPEG';
  return '无视频';
}
function latencyBitrateText() {
  if (shared.activeVideoTransport !== 'webrtc_h264') return '不可用';
  const measured = connectionQuality.bitrateKbps,
    target = adaptive.state?.currentKbps ?? shared.bootstrap?.webrtc?.bitrate_kbps ?? null;
  const parts = [];
  if (measured !== null) parts.push(`实测 ${qualityBitrateText(measured)}`);
  if (target) parts.push(`目标 ${qualityBitrateText(target)}`);
  return parts.length ? parts.join(' / ') : '—';
}
function renderLatencyResult(run) {
  const stats = latencyStats(run.samples),
    ms = (v) => (v === null ? '—' : `${v.toFixed(1)} ms`),
    poll = latencyStats(run.pollMs).median,
    rows = [
      ['测量项', '键鼠回环（Caps Lock LED）'],
      ['有效样本', `${stats.count} / ${stats.attempts}`],
      ['最小值', ms(stats.min)],
      ['中位数', ms(stats.median)],
      ['P95', ms(stats.p95)],
      ['LED 轮询耗时（中位数）', ms(poll)],
      ['键鼠通道', run.channel],
      ['视频传输', run.video],
      ['码率', run.bitrate],
      ['Caps Lock', run.restored === null ? '未发送按键' : run.restored ? 'LED 已回到原状态' : '未能恢复，请手动检查'],
      ['画面延迟', '未测量'],
    ];
  const result = $('#latency-result');
  result.replaceChildren(
    ...rows.flatMap(([label, value]) => {
      const dt = document.createElement('dt'),
        dd = document.createElement('dd');
      dt.textContent = label;
      dd.textContent = value;
      return [dt, dd];
    }),
  );
  result.classList.remove('hidden');
}
function setLatencyRunning(running) {
  $('#latency-start').classList.toggle('hidden', running);
  $('#latency-stop').classList.toggle('hidden', !running);
}
async function startLatencyMeasurement() {
  if (shared.latencyRun) return;
  // These presses would change the case of whatever is being typed.
  if (shared.pasteRun || shared.macroPlayback) {
    toast(tr('正在输入或回放宏，请结束后再测量'), true);
    return;
  }
  const warning = '测量会向被控机连续切换 Caps Lock 共 ${} 次，结束后恢复原状态。被控机当前焦点所在的程序会收到这些按键。确定开始吗？';
  if (!confirm(tr(warning).replace('${}', String(LATENCY_SAMPLES)))) return;
  const run = (shared.latencyRun = {
    cancelled: false,
    samples: [],
    pollMs: [],
    restored: null,
    channel: inputSocketReady() ? 'WebSocket' : 'HTTP',
    video: latencyVideoText(),
    bitrate: '—',
  });
  setLatencyRunning(true);
  $('#latency-result').classList.add('hidden');
  const status = $('#latency-status');
  status.textContent = '';
  let initial = null;
  try {
    initial = await readCapsLock();
    let current = initial,
      misses = 0;
    for (let i = 0; i < LATENCY_SAMPLES && !run.cancelled && misses < LATENCY_MAX_MISSES; i++) {
      status.textContent = `正在测量 ${i + 1} / ${LATENCY_SAMPLES}`;
      const sample = await latencySample(run, current);
      // Stopping mid-sample is not a lost LED report.
      if (sample === null && run.cancelled) break;
      run.samples.push(sample);
      misses = sample === null ? misses + 1 : 0;
      await latencyWait(LATENCY_GAP_MS);
      current = sample === null ? await readCapsLock() : !current;
      // A background tab's throttled timers would inflate every later sample.
      if (document.hidden) run.cancelled = true;
    }
    if (misses >= LATENCY_MAX_MISSES) status.textContent = '被控机没有回报 LED 变化，测量已中止';
    else status.textContent = run.cancelled ? '测量已停止' : '测量完成';
  } catch (err) {
    status.textContent = '测量中断';
    handleHidError(err);
  } finally {
    // Whatever happened, put Caps Lock back the way the user left it.
    if (initial !== null) {
      try {
        // A late report must not trigger a press that undoes the restore.
        await latencyWait(LATENCY_RESTORE_SETTLE_MS);
        if ((await readCapsLock()) !== initial) await tapCapsLock();
        run.restored = await waitForCapsLock(initial);
      } catch (_) {
        run.restored = false;
      }
    }
    run.bitrate = latencyBitrateText();
    const video = latencyVideoText();
    if (video !== run.video) run.video = `${run.video} → ${video}`;
    renderLatencyResult(run);
    shared.latencyRun = null;
    setLatencyRunning(false);
  }
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  $('#latency-start').addEventListener('click', startLatencyMeasurement);
  $('#latency-stop').addEventListener('click', () => {
    if (shared.latencyRun) shared.latencyRun.cancelled = true;
  });
}
