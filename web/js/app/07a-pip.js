  // Picture-in-Picture of the live remote screen. WebRTC uses its <video>
  // directly; MJPEG is an <img>, so it is painted into a canvas whose stream
  // feeds a hidden <video>. A multipart <img> fires no per-frame event, hence
  // the fixed-rate painter. The painter ticks from a worker because timers on
  // a backgrounded page (the point of PiP) are throttled to 1 Hz or less.
  const PIP_FPS = 15;
  let pipVideo = null,
    pipStream = null,
    pipStopTicker = null,
    pipRequest = 0;
  function isPipActive() {
    const element = document.pictureInPictureElement;
    return !!element && (element === webrtcFeed || element === pipVideo);
  }
  function syncPipButton() {
    const button = $('#pip');
    if (!button) return;
    const active = isPipActive();
    button.setAttribute('aria-pressed', String(active));
    button.title = active ? '退出画中画' : '画中画';
    button.setAttribute('aria-label', button.title);
  }
  function startPipTicker(tick) {
    try {
      const url = URL.createObjectURL(
          new Blob([`setInterval(() => postMessage(0), ${1000 / PIP_FPS});`], { type: 'text/javascript' }),
        ),
        worker = new Worker(url);
      worker.onmessage = tick;
      return () => {
        worker.terminate();
        URL.revokeObjectURL(url);
      };
    } catch (_) {
      const timer = setInterval(tick, 1000 / PIP_FPS);
      return () => clearInterval(timer);
    }
  }
  function teardownMjpegPip() {
    pipStopTicker?.();
    pipStopTicker = null;
    pipStream?.getTracks().forEach((track) => track.stop());
    pipStream = null;
    if (pipVideo) {
      pipVideo.srcObject = null;
      pipVideo.remove();
      pipVideo = null;
    }
  }
  function pipEnded() {
    teardownMjpegPip();
    syncPipButton();
    // Closed from the PiP window while the tab is in the background.
    if (videoUiState !== 'paused' && !shouldVideoRun()) stopVideo();
  }
  // Safe to call at any time: leaves PiP and abandons a pending request.
  function closePip() {
    pipRequest++;
    if (isPipActive()) document.exitPictureInPicture().catch(() => {});
    else teardownMjpegPip();
    syncPipButton();
  }
  async function prepareMjpegPip() {
    const canvas = document.createElement('canvas');
    if (!drawLiveFrame(canvas)) throw new Error('no live frame');
    const video = document.createElement('video');
    video.className = 'pip-source';
    video.muted = true;
    video.playsInline = true;
    video.setAttribute('aria-hidden', 'true');
    video.addEventListener('enterpictureinpicture', syncPipButton);
    video.addEventListener('leavepictureinpicture', pipEnded);
    pipStream = canvas.captureStream(PIP_FPS);
    pipVideo = video;
    video.srcObject = pipStream;
    document.body.append(video);
    pipStopTicker = startPipTicker(() => drawLiveFrame(canvas));
    await video.play();
    return video;
  }
  async function togglePip() {
    if (isPipActive()) {
      closePip();
      return;
    }
    if (videoUiState !== 'playing' || !liveVideoSize().width) {
      toast('当前没有可显示的画面', true);
      return;
    }
    const request = ++pipRequest;
    teardownMjpegPip();
    try {
      const video = activeVideoTransport === 'webrtc_h264' ? webrtcFeed : await prepareMjpegPip();
      if (request !== pipRequest) return;
      await video.requestPictureInPicture();
      // Video stopped or switched transport while the browser was entering.
      if (request !== pipRequest) closePip();
    } catch (_) {
      if (request !== pipRequest) return;
      teardownMjpegPip();
      toast('无法进入画中画', true);
    }
    syncPipButton();
  }
  if (document.pictureInPictureEnabled && HTMLVideoElement.prototype.requestPictureInPicture) {
    $('#pip')?.classList.remove('hidden');
    $('#pip')?.addEventListener('click', togglePip);
    webrtcFeed.addEventListener('enterpictureinpicture', syncPipButton);
    webrtcFeed.addEventListener('leavepictureinpicture', pipEnded);
  }
