  function shouldVideoRun() {
    // An active Picture-in-Picture window keeps the picture visible while the
    // tab is in the background, so only then may video outlive a hidden page.
    return !app.classList.contains('hidden') && (!document.hidden || isPipActive()) && videoWorkspace === 'video';
  }
  function showVideoSurface(kind) {
    feed.classList.toggle('hidden', kind !== 'mjpeg');
    webrtcFeed.classList.toggle('hidden', kind !== 'webrtc_h264');
    invalidateSurfaceBox();
  }
  function setVideoMessage(message, visible = true) {
    const box = $('#video-message');
    box.textContent = message;
    box.classList.toggle('hidden', !visible);
  }
  function videoFrameAgeMs() {
    const rawAge = latestVideoStatus?.last_frame_age_ms;
    if (rawAge !== null && rawAge !== undefined && rawAge !== '') {
      const age = Number(rawAge);
      if (Number.isFinite(age) && age >= 0) return age;
    }
    const rawUnix = latestVideoStatus?.last_frame_unix_ms;
    if (rawUnix !== null && rawUnix !== undefined && rawUnix !== '') {
      const unix = Number(rawUnix);
      if (Number.isFinite(unix) && unix > 0) return Math.max(0, Date.now() - unix);
    }
    return null;
  }
  function videoFrameAgeText() {
    const age = videoFrameAgeMs();
    if (age === null) return '';
    if (age < 1000) return '最近有效帧：刚刚';
    if (age < 60000) return `最近有效帧：${Math.round(age / 1000)} 秒前`;
    return `最近有效帧：${Math.round(age / 60000)} 分钟前`;
  }
  // The top bar shows a short state; the full description lives in the
  // tooltip and the diagnostics panel, so long translations never truncate.
  function setStatusLabel(short, full) {
    $('#status-label').textContent = short;
    $('#diagnostics-toggle').title = full;
  }
  function renderVideoUi() {
    syncQualityOverlay();
    const dot = $('#status-dot'),
      label = $('#status-label'),
      meta = $('#video-transport-label'),
      mode =
        activeVideoTransport === 'webrtc_h264'
          ? 'H.264'
          : activeVideoTransport === 'mjpeg'
            ? 'MJPEG'
            : '视频';
    const serverState = latestVideoStatus?.state,
      serverReady = serverState === 'ready',
      serverStarting = serverState === 'starting',
      serverStalled = serverState === 'stalled',
      serverUnavailable = serverState && !serverReady && !serverStarting,
      ageText = videoFrameAgeText();
    dot.classList.toggle('online', videoUiState === 'playing' && serverReady);
    dot.classList.toggle(
      'warning',
      videoUiState === 'connecting' || videoUiState === 'fallback' || serverStarting || serverUnavailable,
    );
    viewport.classList.toggle('video-unhealthy', !!serverState && !serverReady);
    if (!shouldVideoRun()) {
      setStatusLabel('已暂停', '视频已暂停');
      meta.textContent = '暂停';
      return;
    }
    if (serverUnavailable && latestVideoStatus?.reason === 'device_missing') {
      showVideoSurface('none');
      setVideoMessage('采集卡已断开，重新插入后将自动恢复');
      setStatusLabel('已断开', '采集卡已断开');
      meta.textContent = mode;
      return;
    }
    if (serverUnavailable) {
      showVideoSurface('none');
      const reason = latestVideoStatus?.message || (serverStalled ? '视频已停止更新' : '设备离线');
      setVideoMessage(ageText ? `${reason} · ${ageText}` : reason);
      setStatusLabel(serverStalled ? '已停止' : '离线', serverStalled ? '视频已停止更新' : reason);
      meta.textContent = serverStalled ? '已停止更新' : mode;
      return;
    }
    if (serverStarting) {
      showVideoSurface('none');
      setVideoMessage(ageText ? `正在等待首帧… · ${ageText}` : '正在等待首帧…');
      setStatusLabel('等待首帧', `正在等待 ${mode} 首帧…`);
      meta.textContent = mode;
      return;
    }
    if (videoUiState === 'playing' && serverReady) {
      const fps = latestVideoStatus?.frames_per_second;
      showVideoSurface(activeVideoTransport === 'webrtc_h264' ? 'webrtc_h264' : 'mjpeg');
      const live = `${fps ? `${Math.round(fps)} fps` : mode}`;
      setStatusLabel(videoFallback ? `回退 · ${live}` : `实时 · ${live}`, videoFallback ? 'MJPEG · H.264 回退' : `${mode} · LIVE`);
      meta.textContent = `LIVE / ${mode}`;
      setVideoMessage('', false);
      return;
    }
    if (videoUiState === 'fallback') {
      setStatusLabel('回退 MJPEG', 'MJPEG · H.264 回退');
      meta.textContent = 'MJPEG';
      return;
    }
    setStatusLabel('连接中', videoUiState === 'connecting' ? `正在连接 ${mode}…` : '视频连接中');
    meta.textContent = mode;
    updateDiagnosticsCard();
  }
  function setVideoState(state, transport = activeVideoTransport) {
    activeVideoTransport = transport;
    videoUiState = state;
    renderVideoUi();
    updateDiagnosticsCard();
  }
  function clearVideoTimers() {
    clearTimeout(reconnectTimer);
    clearTimeout(videoFirstFrameTimer);
    reconnectTimer = 0;
    videoFirstFrameTimer = 0;
  }
  function closeWebrtcSession(sessionId) {
    if (!sessionId) return;
    fetch('/api/webrtc/close', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ session_id: sessionId }),
      credentials: 'same-origin',
      keepalive: true,
    }).catch(() => {});
  }
  function cleanupWebrtc() {
    const peer = webrtcPeer,
      sessionId = webrtcSessionId,
      abort = webrtcAbort;
    webrtcPeer = null;
    webrtcSessionId = null;
    resetAdaptiveBitrate(null);
    webrtcAbort = null;
    abort?.abort();
    if (peer) {
      peer.ontrack = null;
      peer.onconnectionstatechange = null;
      peer.oniceconnectionstatechange = null;
      peer.getReceivers().forEach((receiver) => receiver.track?.stop());
      peer.close();
    }
    if (sessionId) closeWebrtcSession(sessionId);
    webrtcFeed.onplaying = null;
    webrtcFeed.onloadedmetadata = null;
    webrtcFeed.pause();
    webrtcFeed.srcObject = null;
  }
  function stopVideo(showPaused = true) {
    videoGeneration++;
    clearVideoTimers();
    reconnectDelay = 500;
    closePip();
    cleanupWebrtc();
    feed.onload = null;
    feed.onerror = null;
    feed.removeAttribute('src');
    showVideoSurface('none');
    activeVideoTransport = 'none';
    videoFallback = false;
    videoFallbackReason = '';
    videoUiState = 'paused';
    syncQualityOverlay();
    if (showPaused) renderVideoUi();
    updateDiagnosticsCard();
  }
  function restartVideo() {
    stopVideo(false);
    if (shouldVideoRun()) startVideo();
  }
  function markVideoPlaying(generation, transport) {
    if (generation !== videoGeneration || !shouldVideoRun() || activeVideoTransport !== transport) return;
    clearTimeout(videoFirstFrameTimer);
    setVideoState('playing', transport);
    if (latestVideoStatus?.state === 'ready') setVideoMessage('', false);
  }
  function startMjpeg(generation, fallback = false) {
    if (generation !== videoGeneration || !shouldVideoRun()) return;
    activeVideoTransport = 'mjpeg';
    videoFallback = fallback || videoFallback;
    setVideoState(videoFallback ? 'fallback' : 'connecting', 'mjpeg');
    setVideoMessage(videoFallback ? 'H.264 不可用，切换 MJPEG…' : '正在连接 MJPEG…');
    showVideoSurface('mjpeg');
    feed.onload = null;
    feed.onerror = null;
    feed.removeAttribute('src');
    mjpegLoadEvents = 0;
    const url = `/video_feed?t=${Date.now()}`;
    const onLoad = () => {
      if (generation !== videoGeneration || !shouldVideoRun() || feed.getAttribute('src') !== url) return;
      reconnectDelay = 500;
      mjpegLoadEvents++;
      markVideoPlaying(generation, 'mjpeg');
      fitConsoleToVideo();
    };
    const onError = () => {
      if (generation !== videoGeneration || !shouldVideoRun() || feed.getAttribute('src') !== url) return;
      feed.onload = null;
      feed.onerror = null;
      feed.removeAttribute('src');
      setVideoState(videoFallback ? 'fallback' : 'connecting', 'mjpeg');
      setVideoMessage('视频断开，正在重连…');
      clearTimeout(reconnectTimer);
      const delay = reconnectDelay;
      reconnectTimer = setTimeout(() => {
        if (generation === videoGeneration && shouldVideoRun()) startMjpeg(generation, videoFallback);
      }, delay);
      reconnectDelay = Math.min(reconnectDelay * 2, 10000);
    };
    feed.onload = onLoad;
    feed.onerror = onError;
    feed.setAttribute('src', url);
    clearTimeout(videoFirstFrameTimer);
    videoFirstFrameTimer = setTimeout(onError, 6000);
  }
  function waitForIceGathering(peer, signal, timeout = 2500) {
    if (peer.iceGatheringState === 'complete') return Promise.resolve();
    return new Promise((resolve, reject) => {
      let timer = 0;
      const finish = (error) => {
        clearTimeout(timer);
        peer.removeEventListener('icegatheringstatechange', changed);
        signal?.removeEventListener('abort', aborted);
        error ? reject(error) : resolve();
      };
      const changed = () => {
        if (peer.iceGatheringState === 'complete') finish();
      };
      const aborted = () => finish(new Error('视频连接已取消'));
      peer.addEventListener('icegatheringstatechange', changed);
      signal?.addEventListener('abort', aborted, { once: true });
      timer = setTimeout(() => finish(new Error('WebRTC 本地 ICE 候选收集超时')), timeout);
    });
  }
  function fallbackToMjpeg(generation, reason = 'H.264 连接未能建立') {
    if (generation !== videoGeneration || !shouldVideoRun() || activeVideoTransport !== 'webrtc_h264') return;
    videoFallbackReason = String(reason);
    console.warn('H.264 回退到 MJPEG:', videoFallbackReason);
    closePip();
    cleanupWebrtc();
    startMjpeg(generation, true);
  }
  async function startWebrtc(generation) {
    if (generation !== videoGeneration || !shouldVideoRun()) return;
    activeVideoTransport = 'webrtc_h264';
    setVideoState('connecting', 'webrtc_h264');
    setVideoMessage('正在连接 H.264…');
    showVideoSurface('webrtc_h264');
    const controller = new AbortController();
    webrtcAbort = controller;
    let peer = null,
      signalTimer = 0;
    try {
      peer = new RTCPeerConnection({ iceServers: bootstrap.ice_servers || [] });
      if (generation !== videoGeneration || !shouldVideoRun()) {
        peer.close();
        return;
      }
      webrtcPeer = peer;
      const transceiver = peer.addTransceiver('video', { direction: 'recvonly' });
      if (transceiver?.setCodecPreferences && globalThis.RTCRtpReceiver?.getCapabilities) {
        const codecs = RTCRtpReceiver.getCapabilities('video')?.codecs || [],
          h264 = codecs.filter((codec) => String(codec.mimeType || '').toLowerCase() === 'video/h264');
        if (h264.length)
          transceiver.setCodecPreferences([...h264, ...codecs.filter((codec) => !h264.includes(codec))]);
      }
      peer.ontrack = (event) => {
        if (
          generation !== videoGeneration ||
          !shouldVideoRun() ||
          activeVideoTransport !== 'webrtc_h264' ||
          event.track.kind !== 'video'
        )
          return;
        const stream = event.streams?.[0] || new MediaStream([event.track]);
        webrtcFeed.srcObject = stream;
        webrtcFeed.onplaying = () => markVideoPlaying(generation, 'webrtc_h264');
        webrtcFeed.onloadedmetadata = () => {
          setDisplayScale(displayModes[modeIndex][0]);
          fitConsoleToVideo();
        };
        webrtcFeed.onresize = () => fitConsoleToVideo();
        event.track.onended = () => fallbackToMjpeg(generation, 'H.264 视频轨道已结束');
        showVideoSurface('webrtc_h264');
        webrtcFeed.play().catch(() => {});
      };
      peer.onconnectionstatechange = () => {
        if (generation !== videoGeneration || peer !== webrtcPeer) return;
        if (peer.connectionState === 'failed' || peer.connectionState === 'closed')
          fallbackToMjpeg(generation, `WebRTC 连接状态：${peer.connectionState}`);
        else if (peer.connectionState === 'disconnected') {
          clearTimeout(reconnectTimer);
          reconnectTimer = setTimeout(() => {
            if (
              generation === videoGeneration &&
              peer === webrtcPeer &&
              peer.connectionState === 'disconnected'
            )
              fallbackToMjpeg(generation, 'WebRTC 媒体连接已断开');
          }, 2000);
        } else clearTimeout(reconnectTimer);
      };
      peer.oniceconnectionstatechange = () => {
        if (generation !== videoGeneration || peer !== webrtcPeer) return;
        if (peer.iceConnectionState === 'failed') fallbackToMjpeg(generation, 'WebRTC ICE 连通性检查失败');
      };
      const offer = await peer.createOffer();
      await peer.setLocalDescription(offer);
      await waitForIceGathering(peer, controller.signal);
      if (generation !== videoGeneration || !shouldVideoRun()) return;
      signalTimer = setTimeout(() => controller.abort(), 8000);
      const answer = await request('/api/webrtc/offer', {
        method: 'POST',
        body: JSON.stringify({ type: 'offer', sdp: peer.localDescription?.sdp || '' }),
        signal: controller.signal,
      });
      clearTimeout(signalTimer);
      signalTimer = 0;
      if (generation !== videoGeneration || !shouldVideoRun()) {
        closeWebrtcSession(answer?.session_id);
        return;
      }
      webrtcSessionId = answer?.session_id || null;
      resetAdaptiveBitrate(webrtcSessionId, answer?.bitrate_kbps, answer?.min_bitrate_kbps);
      if (!answer?.sdp || !/H264\/90000/i.test(answer.sdp)) throw new Error('未协商 H.264');
      clearTimeout(videoFirstFrameTimer);
      videoFirstFrameTimer = setTimeout(
        () =>
          fallbackToMjpeg(
            generation,
            `等待 H.264 首帧超时（连接：${peer.connectionState}，ICE：${peer.iceConnectionState}）`,
          ),
        6000,
      );
      await peer.setRemoteDescription({ type: answer.type || 'answer', sdp: answer.sdp });
    } catch (error) {
      clearTimeout(signalTimer);
      if (generation !== videoGeneration || !shouldVideoRun()) {
        if (peer && peer !== webrtcPeer) peer.close();
        return;
      }
      fallbackToMjpeg(
        generation,
        error?.name === 'AbortError' ? 'H.264 信令请求超时' : error?.message || String(error),
      );
    }
  }
  function startVideo(force = false) {
    if (!shouldVideoRun()) return;
    if (force) stopVideo(false);
    else if (videoUiState !== 'paused') return;
    const generation = ++videoGeneration,
      canH264 = h264Available();
    clearVideoTimers();
    videoFallback = false;
    videoFallbackReason =
      videoPreference === 'webrtc_h264' && !canH264
        ? bootstrap.webrtc?.message || '当前浏览器不支持 H.264 WebRTC'
        : '';
    reconnectDelay = 500;
    showVideoSurface('none');
    setVideoMessage('正在连接视频…');
    if (videoPreference === 'mjpeg' || !canH264)
      startMjpeg(generation, videoPreference === 'webrtc_h264' && !canH264);
    else startWebrtc(generation);
  }
  $('#mode-button').addEventListener('click', () => {
    const nextIndex = (modeIndex + 1) % displayModes.length;
    setDisplayScale(displayModes[nextIndex][0], true);
    toast(t('mode_prefix', '画面比例: ') + displayModes[nextIndex][2]);
  });
  async function toggleFullscreen() {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await consoleBox.requestFullscreen();
    } catch (e) {
      toast(e.message, true);
    }
  }
  $$('#fullscreen,[data-viewer-fullscreen]').forEach((button) =>
    button.addEventListener('click', toggleFullscreen),
  );
  // Touch fullscreen hides the console header; this toolbar (CSS shows it only
  // for coarse pointers in fullscreen) keeps keyboard, control and exit at hand.
  $$('[data-touch-action]').forEach((button) =>
    button.addEventListener('click', () => {
      const action = button.dataset.touchAction;
      if (action === 'keyboard') $('#keyboard-toggle')?.click();
      else if (action === 'input') $('#input-state')?.click();
      else if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    }),
  );
  document.addEventListener('fullscreenchange', () => {
    const isFs = !!document.fullscreenElement;
    $$('[data-viewer-fullscreen]').forEach(
      (button) =>
        (button.textContent = isFs ? t('exit_fullscreen', '退出全屏') : t('fullscreen', '进入全屏')),
    );
    const fsBtn = $('#fullscreen');
    if (fsBtn) {
      fsBtn.querySelector('.fullscreen-icon')?.classList.toggle('hidden', isFs);
      fsBtn.querySelector('.fullscreen-exit-icon')?.classList.toggle('hidden', !isFs);
      fsBtn.title = isFs ? t('exit_fullscreen', '退出全屏 (Esc/F11)') : t('fullscreen', '全屏 (F11)');
      fsBtn.setAttribute('aria-label', isFs ? t('exit_fullscreen', '退出全屏') : t('fullscreen', '全屏'));
    }
    if (!isFs) $('#console-head')?.classList.remove('revealed');
  });
  let fsMoveRaf = 0;
  document.addEventListener(
    'pointermove',
    (event) => {
      if (!document.fullscreenElement) return;
      if (fsMoveRaf) return;
      const cy = event.clientY;
      fsMoveRaf = requestAnimationFrame(() => {
        fsMoveRaf = 0;
        if (!document.fullscreenElement) return;
        const head = $('#console-head');
        if (!head) return;
        if (cy < 48) head.classList.add('revealed');
        else if (cy > 84 && !head.matches(':hover') && !head.contains(document.activeElement))
          head.classList.remove('revealed');
      });
    },
    { passive: true },
  );

  let drag = null,
    dragRaf = 0,
    dragArea = null,
    dragBoxWidth = 0,
    dragBoxHeight = 0,
    pendingDragX = 0,
    pendingDragY = 0;
  $('#console-head').addEventListener('pointerdown', (event) => {
    if (
      event.button !== 0 ||
      event.target.closest('button,input,select,a') ||
      matchMedia('(max-width:900px)').matches
    )
      return;
    const r = consoleBox.getBoundingClientRect(),
      w = $('#workspace').getBoundingClientRect();
    drag = {
      x: event.clientX - r.left,
      y: event.clientY - r.top,
      startX: event.clientX,
      startY: event.clientY,
    };
    dragArea = w;
    dragBoxWidth = consoleBox.offsetWidth;
    dragBoxHeight = consoleBox.offsetHeight;
    consoleBox.classList.add('dragging');
    event.currentTarget.setPointerCapture(event.pointerId);
  });
  $('#console-head').addEventListener('pointermove', (event) => {
    if (!drag) return;
    pendingDragX = event.clientX;
    pendingDragY = event.clientY;
    if (Math.abs(event.clientX - drag.startX) > 3 || Math.abs(event.clientY - drag.startY) > 3)
      consoleUserMoved = true;
    if (!dragRaf) {
      dragRaf = requestAnimationFrame(() => {
        dragRaf = 0;
        if (!drag || !dragArea) return;
        const left = Math.max(
          0,
          Math.min(pendingDragX - dragArea.left - drag.x, dragArea.width - dragBoxWidth),
        );
        const top = Math.max(
          0,
          Math.min(pendingDragY - dragArea.top - drag.y, dragArea.height - dragBoxHeight),
        );
        consoleBox.style.left = `${left}px`;
        consoleBox.style.top = `${top}px`;
        invalidateSurfaceBox();
      });
    }
  });
  const stopDrag = () => {
    if (drag) {
      drag = null;
      if (dragRaf) {
        cancelAnimationFrame(dragRaf);
        dragRaf = 0;
      }
      consoleBox.classList.remove('dragging');
      invalidateSurfaceBox();
    }
  };
  $('#console-head').addEventListener('pointerup', stopDrag);
  $('#console-head').addEventListener('pointercancel', stopDrag);
  $('#console-head').addEventListener('dblclick', (event) => {
    if (event.target.closest('button,input,select,a')) return;
    consoleUserMoved = false;
    fitConsoleToVideo(true);
  });


  // Screenshot and recording of the remote screen. Both work on whichever
  // surface is live (MJPEG <img> or WebRTC <video>) and stay in the browser:
  // nothing is uploaded, the file is downloaded locally.
  function downloadBlob(blob, name) {
    const url = URL.createObjectURL(blob),
      link = document.createElement('a');
    link.href = url;
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  function captureStamp() {
    return new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  }
  function liveVideoSize() {
    const surface = activeVideoTransport === 'webrtc_h264' ? webrtcFeed : feed;
    const width = surface instanceof HTMLVideoElement ? surface.videoWidth : surface.naturalWidth,
      height = surface instanceof HTMLVideoElement ? surface.videoHeight : surface.naturalHeight;
    return { surface, width, height };
  }
  function drawLiveFrame(canvas) {
    const { surface, width, height } = liveVideoSize();
    if (!width || !height || videoUiState !== 'playing') return false;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    canvas.getContext('2d').drawImage(surface, 0, 0, width, height);
    return true;
  }
  $('#screenshot')?.addEventListener('click', () => {
    const canvas = document.createElement('canvas');
    if (!drawLiveFrame(canvas)) {
      toast('当前没有可截取的画面', true);
      return;
    }
    canvas.toBlob((blob) => {
      if (!blob) {
        toast('截图失败', true);
        return;
      }
      downloadBlob(blob, `wingmankvm-${captureStamp()}.png`);
      toast('已保存截图');
    }, 'image/png');
  });

  // Recording draws frames into a canvas so MJPEG (an <img>) and WebRTC are
  // recorded the same way, at the source resolution.
  const RECORD_FPS = 15;
  let recorder = null,
    recordTimer = 0,
    recordStarted = 0;
  function syncRecordButton() {
    const button = $('#record'),
      recording = !!recorder;
    if (!button) return;
    button.classList.toggle('recording', recording);
    button.setAttribute('aria-pressed', String(recording));
    const seconds = recording ? Math.floor((performance.now() - recordStarted) / 1000) : 0;
    $('.record-text', button).textContent = recording
      ? `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
      : '';
  }
  function stopRecording() {
    if (recorder && recorder.state !== 'inactive') recorder.stop();
  }
  function startRecording() {
    const canvas = document.createElement('canvas');
    if (!drawLiveFrame(canvas)) {
      toast('当前没有可录制的画面', true);
      return;
    }
    const type = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm'].find(
      (candidate) => globalThis.MediaRecorder?.isTypeSupported?.(candidate),
    );
    if (!type) {
      toast('当前浏览器不支持录制', true);
      return;
    }
    const chunks = [],
      stream = canvas.captureStream(RECORD_FPS),
      painter = setInterval(() => drawLiveFrame(canvas), 1000 / RECORD_FPS);
    recorder = new MediaRecorder(stream, { mimeType: type });
    recorder.ondataavailable = (event) => {
      if (event.data.size) chunks.push(event.data);
    };
    recorder.onstop = () => {
      clearInterval(painter);
      clearInterval(recordTimer);
      stream.getTracks().forEach((track) => track.stop());
      recorder = null;
      syncRecordButton();
      if (chunks.length) {
        downloadBlob(new Blob(chunks, { type: 'video/webm' }), `wingmankvm-${captureStamp()}.webm`);
        toast('录制已保存');
      }
    };
    recorder.start(1000);
    recordStarted = performance.now();
    recordTimer = setInterval(syncRecordButton, 1000);
    syncRecordButton();
    toast('开始录制，再次点击停止');
  }
  $('#record')?.addEventListener('click', () => (recorder ? stopRecording() : startRecording()));
  window.addEventListener('pagehide', stopRecording);
