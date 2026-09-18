pub const HTML: &str = r#"
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Remote Microphone</title>
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link
      href="https://fonts.googleapis.com/css2?family=DM+Mono:wght@300;400;500&family=Instrument+Serif:ital@0;1&display=swap"
      rel="stylesheet"
    />
    <style>
      *,
      *::before,
      *::after {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
      }
      :root {
        --bg: #f5f3ee;
        --surface: #faf9f6;
        --border: #ddd9d0;
        --text: #1a1816;
        --muted: #6f6a61;
        --accent: #2a2420;
        --ok: #2d6a4f;
        --ok-bg: #d8f3dc;
        --warn: #b5500a;
        --warn-bg: #fde8d0;
        --err: #9b2222;
        --err-bg: #fdd;
        --radius: 10px;
      }
      @media (prefers-color-scheme: dark) {
        :root {
          --bg: #131210;
          --surface: #1c1a17;
          --border: #2e2b26;
          --text: #e8e4dc;
          --muted: #aaa49a;
          --accent: #e8e4dc;
          --ok: #52b788;
          --ok-bg: #0e2e1e;
          --warn: #f4a261;
          --warn-bg: #2b1800;
          --err: #e07070;
          --err-bg: #2b0d0d;
        }
      }
      body {
        font-family: "DM Mono", monospace;
        background: var(--bg);
        color: var(--text);
        min-height: 100dvh;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        padding: 24px;
      }
      .card {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 16px;
        padding: 36px 32px 28px;
        width: 100%;
        max-width: 360px;
        display: flex;
        flex-direction: column;
        gap: 20px;
        position: relative;
        overflow: hidden;
      }
      .card::before {
        content: "";
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 2px;
        background: linear-gradient(
          90deg,
          transparent,
          var(--border),
          transparent
        );
      }
      .header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
      }
      .wordmark {
        display: flex;
        flex-direction: column;
        gap: 3px;
      }
      .wordmark h1 {
        font-family: "Instrument Serif", serif;
        font-style: italic;
        font-size: 1.6rem;
        font-weight: 400;
        letter-spacing: -0.01em;
        color: var(--text);
        line-height: 1;
      }
      .wordmark .tagline {
        font-size: 0.68rem;
        color: var(--muted);
        letter-spacing: 0.06em;
        text-transform: uppercase;
        font-weight: 300;
      }
      .intro {
        display: flex;
        flex-direction: column;
        gap: 6px;
      }
      .intro h2 {
        font-family: "Instrument Serif", serif;
        font-size: 1.55rem;
        font-weight: 400;
        line-height: 1.08;
        letter-spacing: -0.01em;
      }
      .intro p {
        color: var(--muted);
        font-size: 0.72rem;
        line-height: 1.65;
      }
      .mic-glyph {
        width: 40px;
        height: 40px;
        border: 1px solid var(--border);
        border-radius: var(--radius);
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
      }
      .mic-glyph svg {
        width: 20px;
        height: 20px;
        stroke: var(--muted);
        fill: none;
        stroke-width: 1.5;
        stroke-linecap: round;
        stroke-linejoin: round;
      }
      .divider {
        height: 1px;
        background: var(--border);
        margin: -4px 0;
      }
      .status-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
      }
      .status-panel {
        display: flex;
        flex-direction: column;
        gap: 14px;
        padding: 14px;
        border: 1px solid var(--border);
        border-radius: var(--radius);
        background: color-mix(in srgb, var(--surface) 88%, var(--bg));
      }
      .session-facts {
        display: grid;
        gap: 8px;
        padding-top: 12px;
        border-top: 1px solid var(--border);
      }
      .fact {
        display: grid;
        grid-template-columns: 58px 1fr;
        align-items: baseline;
        gap: 10px;
        font-size: 0.68rem;
        line-height: 1.4;
      }
      .fact-label {
        color: var(--muted);
        text-transform: uppercase;
        letter-spacing: 0.07em;
        font-size: 0.61rem;
      }
      .fact-value {
        color: var(--text);
        text-align: right;
      }
      .status-label {
        font-size: 0.65rem;
        letter-spacing: 0.1em;
        text-transform: uppercase;
        color: var(--muted);
      }
      .badge {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 3px 10px 3px 8px;
        border-radius: 999px;
        font-size: 0.7rem;
        letter-spacing: 0.04em;
        border: 1px solid var(--border);
        color: var(--muted);
        background: transparent;
        transition: all 0.2s;
        max-width: 72%;
        text-align: right;
      }
      .badge .dot {
        width: 5px;
        height: 5px;
        border-radius: 50%;
        background: var(--muted);
        flex-shrink: 0;
        transition: background 0.2s;
      }
      .badge.connecting {
        border-color: var(--warn);
        color: var(--warn);
      }
      .badge.connecting .dot {
        background: var(--warn);
        animation: blink 1s infinite;
      }
      .badge.connected {
        border-color: var(--ok);
        color: var(--ok);
        background: var(--ok-bg);
      }
      .badge.connected .dot {
        background: var(--ok);
      }
      .badge.muted {
        border-color: var(--warn);
        color: var(--warn);
        background: var(--warn-bg);
      }
      .badge.muted .dot {
        background: var(--warn);
      }
      .badge.error {
        border-color: var(--err);
        color: var(--err);
        background: var(--err-bg);
      }
      .badge.error .dot {
        background: var(--err);
      }
      @keyframes blink {
        0%,
        100% {
          opacity: 1;
        }
        50% {
          opacity: 0.25;
        }
      }
      .warn-box {
        display: none;
        background: var(--warn-bg);
        border: 1px solid var(--warn);
        border-radius: var(--radius);
        padding: 12px 14px;
        font-size: 0.72rem;
        line-height: 1.7;
        color: var(--warn);
      }
      .warn-box.visible {
        display: block;
      }
      .warn-box code {
        font-family: "DM Mono", monospace;
        font-size: 0.68rem;
        background: rgba(0, 0, 0, 0.08);
        padding: 1px 5px;
        border-radius: 4px;
      }
      .meter-wrap {
        height: 2px;
        background: var(--border);
        border-radius: 1px;
        overflow: hidden;
        opacity: 0;
        transition: opacity 0.3s;
      }
      .meter-wrap.active {
        opacity: 1;
      }
      .meter-fill {
        height: 100%;
        width: 0%;
        background: var(--ok);
        border-radius: 1px;
        transition: width 0.05s linear;
      }
      .meter-fill.muted {
        background: var(--warn);
      }
      .bars {
        display: flex;
        align-items: flex-end;
        gap: 3px;
        height: 28px;
        opacity: 0;
        transition: opacity 0.4s;
      }
      .bars.active {
        opacity: 1;
      }
      .bars span {
        flex: 1;
        background: var(--border);
        border-radius: 2px;
        height: 4px;
        transition:
          height 0.05s,
          background 0.2s;
      }
      .bars.live span {
        background: var(--ok);
      }
      .bars.muted span {
        background: var(--warn) !important;
      }
      .btn-row {
        display: flex;
        gap: 10px;
      }
      button#btn {
        flex: 1;
        min-height: 50px;
        padding: 13px 16px;
        font-family: "DM Mono", monospace;
        font-size: 0.78rem;
        font-weight: 500;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        border-radius: var(--radius);
        border: 1px solid var(--accent);
        background: var(--accent);
        color: var(--bg);
        cursor: pointer;
        transition:
          opacity 0.15s,
          transform 0.1s;
        -webkit-tap-highlight-color: transparent;
      }
      button#btn:hover {
        opacity: 0.85;
      }
      button#btn:active {
        transform: scale(0.98);
      }
      button:focus-visible {
        outline: 3px solid color-mix(in srgb, var(--ok) 45%, transparent);
        outline-offset: 3px;
      }
      button#btn:disabled {
        opacity: 0.3;
        cursor: not-allowed;
        transform: none;
      }
      button#btn.disconnect {
        background: transparent;
        color: var(--err);
        border-color: var(--err);
      }
      button#btn.disconnect:hover {
        background: var(--err-bg);
      }
      button#mute-btn {
        min-height: 50px;
        padding: 12px 16px;
        font-family: "DM Mono", monospace;
        font-size: 0.78rem;
        font-weight: 500;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        border-radius: var(--radius);
        border: 1px solid var(--border);
        background: transparent;
        color: var(--muted);
        cursor: pointer;
        transition:
          opacity 0.15s,
          transform 0.1s,
          background 0.15s,
          color 0.15s,
          border-color 0.15s;
        -webkit-tap-highlight-color: transparent;
        display: none;
        align-items: center;
        justify-content: center;
        gap: 6px;
      }
      button#mute-btn:hover {
        background: var(--warn-bg);
        color: var(--warn);
        border-color: var(--warn);
      }
      button#mute-btn:active {
        transform: scale(0.98);
      }
      button#mute-btn.visible {
        display: flex;
      }
      button#mute-btn.muted {
        background: var(--warn-bg);
        color: var(--warn);
        border-color: var(--warn);
      }
      button#mute-btn.muted:hover {
        opacity: 0.8;
      }
      .hint {
        font-size: 0.67rem;
        color: var(--muted);
        text-align: center;
        line-height: 1.7;
        letter-spacing: 0.02em;
      }
      @media (max-width: 420px) {
        body {
          justify-content: flex-start;
          padding: 16px;
        }
        .card {
          margin: max(8px, env(safe-area-inset-top)) 0
            max(8px, env(safe-area-inset-bottom));
          padding: 28px 22px 22px;
          gap: 18px;
        }
        .btn-row {
          flex-direction: column;
        }
        button#mute-btn.visible {
          width: 100%;
        }
      }
    </style>
  </head>
  <body>
    <div class="card">
      <div class="header">
        <div class="wordmark">
          <h1>Remote Microphone</h1>
          <span class="tagline">Device → PC microphone</span>
        </div>
        <div class="mic-glyph">
          <svg viewBox="0 0 24 24">
            <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
            <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
            <line x1="12" y1="19" x2="12" y2="23" />
            <line x1="8" y1="23" x2="16" y2="23" />
          </svg>
        </div>
      </div>

      <div class="divider"></div>

      <div class="intro">
        <h2>Use this phone as your computer’s microphone.</h2>
        <p>
          Connect once, allow microphone access, then keep this page open while
          you talk.
        </p>
      </div>

      <div class="status-panel" aria-live="polite">
        <div class="status-row">
          <span class="status-label">Connection</span>
          <div id="badge" class="badge">
            <span class="dot"></span>
            <span id="badge-text">Ready</span>
          </div>
        </div>
        <div class="session-facts">
          <div class="fact">
            <span class="fact-label">Mic</span>
            <span class="fact-value" id="mic-state">Off</span>
          </div>
          <div class="fact">
            <span class="fact-label">Audio</span>
            <span class="fact-value" id="stream-state">Not sending</span>
          </div>
          <div class="fact">
            <span class="fact-label">Screen</span>
            <span class="fact-value" id="wake-state">Managed when connected</span>
          </div>
          <div class="fact">
            <span class="fact-label">Quality</span>
            <span class="fact-value">__REMOTEMIC_QUALITY__</span>
          </div>
        </div>
      </div>

      <div id="https-warning" class="warn-box">
        <strong>A secure link is needed</strong><br />
        Your browser will only share the microphone over HTTPS. On the computer,
        run <code id="tunnel-command">npx localtunnel</code>, then open its
        <code>https://…</code> link here.
      </div>

      <div id="wake-warning" class="warn-box">
        <strong>Keep the screen on</strong><br />
        This browser cannot prevent sleep. Leave this page visible and do not
        lock the phone while streaming.
      </div>

      <div class="bars" id="bars">
        <span></span><span></span><span></span><span></span><span></span>
        <span></span><span></span><span></span><span></span><span></span>
        <span></span><span></span><span></span><span></span><span></span>
      </div>

      <div class="meter-wrap" id="meter-wrap">
        <div class="meter-fill" id="meter-fill"></div>
      </div>

      <div class="btn-row">
        <button id="btn">Connect microphone</button>
        <button id="mute-btn" aria-pressed="false">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
            <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
            <line x1="12" y1="19" x2="12" y2="23" />
            <line x1="8" y1="23" x2="16" y2="23" />
          </svg>
          Mute
        </button>
      </div>

      <p class="hint">
        Your microphone is used only while this page is connected.
      </p>
    </div>

    <script>
      "use strict";

      const btn = document.getElementById("btn");
      const muteBtn = document.getElementById("mute-btn");
      const badge = document.getElementById("badge");
      const badgeText = document.getElementById("badge-text");
      const meterWrap = document.getElementById("meter-wrap");
      const meterFill = document.getElementById("meter-fill");
      const httpsWarn = document.getElementById("https-warning");
      const wakeWarn = document.getElementById("wake-warning");
      const tunnelCommand = document.getElementById("tunnel-command");
      const micState = document.getElementById("mic-state");
      const streamState = document.getElementById("stream-state");
      const wakeState = document.getElementById("wake-state");
      const barsEl = document.getElementById("bars");
      const barSpans = barsEl.querySelectorAll("span");

      const SAMPLE_RATE = __REMOTEMIC_SAMPLE_RATE__;
      const SAMPLE_FORMAT = "__REMOTEMIC_SAMPLE_FORMAT__";
      const BYTES_PER_SAMPLE = __REMOTEMIC_BYTES_PER_SAMPLE__;
      const BUFFER_SIZE = SAMPLE_RATE <= 16000 ? 1024 : 4096;
      const MAX_BUFFERED_BYTES = BUFFER_SIZE * BYTES_PER_SAMPLE * 4;
      const TOKEN = "__REMOTEMIC_TOKEN__";

      let currentSession = null;
      let nextSessionId = 0;
      let previousSocketClosed = Promise.resolve();

      const isSecureContext = window.isSecureContext;
      if (!isSecureContext) {
        httpsWarn.classList.add("visible");
        if (location.port)
          tunnelCommand.textContent = `npx localtunnel --port ${location.port}`;
      }

      function setStatus(text, cls) {
        badge.className = "badge " + (cls || "");
        badgeText.textContent = text;
      }

      function wsUrl() {
        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        return `${proto}//${location.host}/ws?token=${encodeURIComponent(TOKEN)}`;
      }

      function isCurrent(session) {
        return currentSession === session && !session.cleaned;
      }

      function setDetails(mic, audio, screen) {
        micState.textContent = mic;
        streamState.textContent = audio;
        wakeState.textContent = screen;
      }

      function resetUi(statusText = "Ready", statusClass = "") {
        setStatus(statusText, statusClass);
        setDetails("Off", "Not sending", "Managed when connected");
        btn.textContent = "Connect microphone";
        btn.classList.remove("disconnect");
        btn.disabled = false;
        muteBtn.classList.remove("visible", "muted");
        muteBtn.setAttribute("aria-pressed", "false");
        wakeWarn.classList.remove("visible");
      }

      function applyMuteVisuals(session) {
        if (!isCurrent(session)) return;
        muteBtn.setAttribute("aria-pressed", String(session.muted));

        if (session.muted) {
          muteBtn.textContent = "";
          const svg = document.createElementNS(
            "http://www.w3.org/2000/svg",
            "svg",
          );
          svg.setAttribute("width", "14");
          svg.setAttribute("height", "14");
          svg.setAttribute("viewBox", "0 0 24 24");
          svg.setAttribute("fill", "none");
          svg.setAttribute("stroke", "currentColor");
          svg.setAttribute("stroke-width", "1.8");
          svg.setAttribute("stroke-linecap", "round");
          svg.setAttribute("stroke-linejoin", "round");
          svg.innerHTML =
            '<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/><line x1="2" y1="2" x2="22" y2="22"/>';
          muteBtn.appendChild(svg);
          muteBtn.appendChild(document.createTextNode(" Unmute"));
          muteBtn.classList.add("muted");
          barsEl.classList.add("muted");
          meterFill.classList.add("muted");
          setStatus("Muted", "muted");
          setDetails("Muted", "Paused — still connected", wakeState.textContent);
        } else {
          muteBtn.innerHTML = "";
          const svg = document.createElementNS(
            "http://www.w3.org/2000/svg",
            "svg",
          );
          svg.setAttribute("width", "14");
          svg.setAttribute("height", "14");
          svg.setAttribute("viewBox", "0 0 24 24");
          svg.setAttribute("fill", "none");
          svg.setAttribute("stroke", "currentColor");
          svg.setAttribute("stroke-width", "1.8");
          svg.setAttribute("stroke-linecap", "round");
          svg.setAttribute("stroke-linejoin", "round");
          svg.innerHTML =
            '<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/>';
          muteBtn.appendChild(svg);
          muteBtn.appendChild(document.createTextNode(" Mute"));
          muteBtn.classList.remove("muted");
          barsEl.classList.remove("muted");
          meterFill.classList.remove("muted");
          setStatus("Streaming audio", "connected");
          setDetails("On", "Sending to computer", wakeState.textContent);
        }
      }

      async function startAudio(session) {
        if (!isSecureContext)
          throw new Error("Requires HTTPS. See warning above.");
        if (!navigator.mediaDevices?.getUserMedia)
          throw new Error("getUserMedia not available.");

        const mediaStream = await navigator.mediaDevices.getUserMedia({
          audio: {
            sampleRate: SAMPLE_RATE,
            channelCount: 1,
            echoCancellation: false,
            noiseSuppression: false,
            autoGainControl: false,
          },
        });

        if (!isCurrent(session)) {
          mediaStream.getTracks().forEach((track) => track.stop());
          throw new Error("Connection cancelled.");
        }

        session.stream = mediaStream;
        micState.textContent = "On";
        const audioCtx = session.audioCtx;
        await audioCtx.resume();
        if (!isCurrent(session)) throw new Error("Connection cancelled.");
        if (audioCtx.sampleRate !== SAMPLE_RATE)
          console.info(
            `Resampling microphone from ${audioCtx.sampleRate} Hz to ${SAMPLE_RATE} Hz`,
          );

        session.source = audioCtx.createMediaStreamSource(mediaStream);

        session.analyser = audioCtx.createAnalyser();
        session.analyser.fftSize = 64;
        session.analyser.smoothingTimeConstant = 0.7;

        session.processor = audioCtx.createScriptProcessor(BUFFER_SIZE, 1, 1);
        session.processor.onaudioprocess = (ev) => {
          const socket = session.socket;
          if (!isCurrent(session) || session.muted) return;
          if (!socket || socket.readyState !== WebSocket.OPEN) return;
          if (socket.bufferedAmount > MAX_BUFFERED_BYTES) {
            session.droppedFrames++;
            return;
          }

          const f32 = ev.inputBuffer.getChannelData(0);
          const pcm = resample(f32, audioCtx.sampleRate, session.resampler);
          const buffer = new ArrayBuffer(pcm.length * BYTES_PER_SAMPLE);
          const view = new DataView(buffer);
          for (let i = 0; i < pcm.length; i++) {
            const c = Math.max(-1, Math.min(1, pcm[i]));
            if (SAMPLE_FORMAT === "float32le") {
              view.setFloat32(i * 4, c, true);
            } else {
              const sample = c < 0 ? c * 0x8000 : c * 0x7fff;
              view.setInt16(i * 2, sample, true);
            }
          }
          socket.send(buffer);
        };

        session.source.connect(session.analyser);
        session.source.connect(session.processor);
        session.processor.connect(audioCtx.destination);

        startMeter(session);
      }

      function resample(input, inputRate, state) {
        if (input.length === 0) return input;
        if (inputRate === SAMPLE_RATE) return input;

        const step = inputRate / SAMPLE_RATE;
        const output = [];
        let position = state.position;
        while (position < input.length - 1) {
          const index = Math.floor(position);
          const fraction = position - index;
          const left = index < 0 ? state.previous : input[index];
          const right = input[index + 1];
          output.push(left + (right - left) * fraction);
          position += step;
        }
        state.position = position - input.length;
        state.previous = input[input.length - 1];
        return output;
      }

      function startMeter(session) {
        meterWrap.classList.add("active");
        barsEl.classList.add("active", "live");

        const timeBuf = new Uint8Array(session.analyser.frequencyBinCount);
        const freqBuf = new Uint8Array(session.analyser.frequencyBinCount);

        function tick() {
          if (!isCurrent(session)) return;
          session.analyser.getByteTimeDomainData(timeBuf);
          let peak = 0;
          for (let i = 0; i < timeBuf.length; i++)
            peak = Math.max(peak, Math.abs(timeBuf[i] - 128));
          meterFill.style.width = Math.min(100, (peak / 128) * 200) + "%";

          session.analyser.getByteFrequencyData(freqBuf);
          const step = Math.floor(freqBuf.length / barSpans.length);
          barSpans.forEach((bar, i) => {
            const val = session.muted ? 0 : freqBuf[i * step] || 0;
            bar.style.height = 4 + (val / 255) * 22 + "px";
          });

          session.meterRaf = requestAnimationFrame(tick);
        }
        session.meterRaf = requestAnimationFrame(tick);
      }

      function stopMeter(session) {
        if (session.meterRaf) cancelAnimationFrame(session.meterRaf);
        session.meterRaf = null;
        meterWrap.classList.remove("active");
        meterFill.style.width = "0%";
        barsEl.classList.remove("active", "live", "muted");
        barSpans.forEach((b) => (b.style.height = "4px"));
      }

      async function requestWakeLock(session) {
        if (!isCurrent(session) || document.visibilityState !== "visible") return;
        if (!("wakeLock" in navigator)) {
          wakeWarn.classList.add("visible");
          wakeState.textContent = "Keep on manually";
          return;
        }
        if (session.wakeLock && !session.wakeLock.released) return;
        if (session.wakeLockRequest) return session.wakeLockRequest;

        const request = navigator.wakeLock.request("screen");
        session.wakeLockRequest = request;
        try {
          const sentinel = await request;
          if (!isCurrent(session)) {
            await sentinel.release();
            return;
          }
          session.wakeLock = sentinel;
          wakeWarn.classList.remove("visible");
          wakeState.textContent = "Stays awake while connected";
          sentinel.addEventListener("release", () => {
            if (session.wakeLock === sentinel) session.wakeLock = null;
            if (isCurrent(session)) wakeState.textContent = "Paused while hidden";
          });
        } catch (err) {
          if (isCurrent(session)) {
            console.warn("Screen wake lock unavailable:", err);
            wakeWarn.classList.add("visible");
            wakeState.textContent = "Keep on manually";
          }
        } finally {
          if (session.wakeLockRequest === request)
            session.wakeLockRequest = null;
        }
      }

      function waitForSocketClose(session) {
        const socket = session.socket;
        if (!socket || socket.readyState === WebSocket.CLOSED)
          return Promise.resolve();

        return new Promise((resolve) => {
          session.closeResolvers.push(resolve);
          if (socket.readyState < WebSocket.CLOSING) socket.close();
          setTimeout(resolve, 1500);
        });
      }

      function cleanup(session, closeSocket = false) {
        if (closeSocket) previousSocketClosed = waitForSocketClose(session);
        if (session.cleaned) return previousSocketClosed;
        session.cleaned = true;

        stopMeter(session);
        if (session.processor) {
          session.processor.onaudioprocess = null;
          session.processor.disconnect();
        }
        session.source?.disconnect();
        session.stream?.getTracks().forEach((track) => track.stop());
        session.audioCtx?.close().catch(() => {});
        session.wakeLock?.release().catch(() => {});
        if (session.connectTimer) clearTimeout(session.connectTimer);
        if (session.droppedFrames > 0)
          console.warn(
            `Dropped ${session.droppedFrames} audio frames to avoid latency`,
          );

        session.processor = null;
        session.source = null;
        session.analyser = null;
        session.stream = null;
        session.audioCtx = null;
        session.wakeLock = null;
        return previousSocketClosed;
      }

      function connect() {
        if (currentSession) return;

        const AudioContextClass = window.AudioContext || window.webkitAudioContext;
        if (!AudioContextClass) {
          setStatus("Web Audio unavailable", "error");
          return;
        }

        const session = {
          id: ++nextSessionId,
          socket: null,
          stream: null,
          audioCtx: new AudioContextClass({ sampleRate: SAMPLE_RATE }),
          source: null,
          processor: null,
          analyser: null,
          meterRaf: null,
          wakeLock: null,
          wakeLockRequest: null,
          closeResolvers: [],
          resampler: { position: 0, previous: 0 },
          muted: false,
          cleaned: false,
          accepted: false,
          droppedFrames: 0,
          errorMessage: null,
          connectTimer: null,
        };
        currentSession = session;

        btn.textContent = "Cancel";
        btn.classList.add("disconnect");
        btn.disabled = false;
        setStatus("Connecting…", "connecting");
        setDetails("Waiting for permission", "Connecting", "Requesting wake lock");
        session.audioCtx.resume().catch(() => {});
        requestWakeLock(session);

        const priorClose = previousSocketClosed;
        (async () => {
          await priorClose;
          if (!isCurrent(session)) return;

          const socket = new WebSocket(wsUrl());
          session.socket = socket;
          socket.binaryType = "arraybuffer";
          session.connectTimer = setTimeout(() => {
            if (!isCurrent(session) || session.accepted) return;
            session.errorMessage = "Computer did not respond";
            setStatus(session.errorMessage, "error");
            socket.close();
          }, 8000);

          socket.onmessage = async ({ data }) => {
            if (typeof data !== "string" || !isCurrent(session)) return;
            console.info("Server:", data);
            if (data.startsWith("error:")) {
              session.errorMessage = "Another device is already connected";
              setStatus(session.errorMessage, "error");
              socket.close();
              return;
            }
            if (data !== "ok: connected" || session.accepted) return;

            session.accepted = true;
            clearTimeout(session.connectTimer);
            session.connectTimer = null;
            try {
              await startAudio(session);
              if (!isCurrent(session)) return;
              setStatus("Streaming audio", "connected");
              streamState.textContent = "Sending to computer";
              btn.textContent = "Disconnect microphone";
              btn.classList.add("disconnect");
              btn.disabled = false;
              muteBtn.classList.add("visible");
            } catch (err) {
              if (!isCurrent(session)) return;
              session.errorMessage = friendlyAudioError(err);
              setStatus(session.errorMessage, "error");
              socket.close();
            }
          };

          socket.onclose = () => {
            session.closeResolvers.splice(0).forEach((resolve) => resolve());
            cleanup(session);
            if (currentSession !== session) return;
            currentSession = null;
            resetUi(
              session.errorMessage
                ? session.errorMessage
                : "Ready",
              session.errorMessage ? "error" : "",
            );
          };

          socket.onerror = () => {
            if (isCurrent(session)) setStatus("Could not reach computer", "error");
          };
        })().catch((err) => {
          if (!isCurrent(session)) return;
          session.errorMessage = err.message;
          currentSession = null;
          cleanup(session, true);
          resetUi("Could not connect", "error");
        });
      }

      function friendlyAudioError(err) {
        if (err?.name === "NotAllowedError") return "Microphone permission denied";
        if (err?.name === "NotFoundError") return "No microphone found";
        if (err?.name === "NotReadableError") return "Microphone is busy";
        return "Microphone unavailable";
      }

      function disconnect() {
        const session = currentSession;
        if (!session) return;
        currentSession = null;
        cleanup(session, true);
        resetUi();
      }

      muteBtn.addEventListener("click", () => {
        const session = currentSession;
        if (!session || !session.accepted) return;
        session.muted = !session.muted;
        if (!session.muted) session.resampler = { position: 0, previous: 0 };
        applyMuteVisuals(session);
      });

      btn.addEventListener("click", () => {
        if (currentSession) disconnect();
        else connect();
      });

      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState === "visible" && currentSession)
          requestWakeLock(currentSession);
      });
    </script>
  </body>
</html>
"#;
