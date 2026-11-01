// Mutable state shared by more than one module. An imported binding is
// read-only in the importing module, so every variable that two or more
// modules read or assign lives here as a property of one object. State used
// by a single module stays a module-local `let` in that module.
export const shared = {
  // Console placement and native-size panning (02, 04, 07, 08, 08b).
  cachedSurfaceBox: null,
  consoleUserMoved: false,
  isPanActive: false,
  isDraggingPan: false,

  // Bootstrap data from /api/bootstrap (most modules) and page visibility.
  bootstrap: {},
  pageActive: !document.hidden,

  // Video transport and its UI (03-session, 04, 05, 07-video, 07a-07d, 08).
  videoWorkspace: 'video',
  videoPreference: 'auto',
  activeVideoTransport: 'none',
  videoUiState: 'paused',
  videoFallbackReason: '',
  webrtcPeer: null,
  webrtcSessionId: null,
  modeIndex: 0,
  mjpegLoadEvents: 0,

  // Latest /api/status snapshots (10-workspace-status writes them).
  latestVideoStatus: null,
  latestPowerStatus: null,
  latestSystemStatus: null,

  // HDMI audio rides on the WebRTC session: `webrtcAudio` is null until the
  // answer says whether an Opus track was negotiated. Playback always starts
  // muted (browser autoplay rules); the user unmutes explicitly.
  webrtcAudio: null,
  audioMuted: true,
  latestAudioStatus: null,

  // Absolute pointer position and the pending move (08-input, 08b-touch).
  mouseX: 0,
  mouseY: 0,
  absolutePending: null,

  // Long-running input jobs that must not overlap (07z, 08, 08c): the
  // latency tool's Caps Lock presses would flip the case of typed text.
  macroPlayback: null,
  pasteRun: null,
  latencyRun: null,

  // Terminal workspace (09-terminal, 10-workspace-status).
  terminalSocket: null,
  terminal: null,
};
