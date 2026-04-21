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
        --muted: #8c887f;
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
          --muted: #6e6a62;
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
        padding: 12px;
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
        padding: 12px 14px;
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

      <div class="status-row">
        <span class="status-label">Status</span>
        <div id="badge" class="badge">
          <span class="dot"></span>
          <span id="badge-text">Disconnected</span>
        </div>
      </div>

      <div id="https-warning" class="warn-box">
        <strong>Microphone blocked</strong><br />
        Browsers only allow mic access on <code>localhost</code> or
        <code>https://</code>.<br /><br />
        Fix: run on your PC:<br />
        <code>npx localtunnel --port 8080</code><br />
        Then open the <code>https://…</code> link on your phone.
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
        <button id="btn">Connect</button>
        <button id="mute-btn">
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
        Open on your device · tap Connect · audio streams to your PC
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
      const barsEl = document.getElementById("bars");
      const barSpans = barsEl.querySelectorAll("span");

      const SAMPLE_RATE = 44100;
      const BUFFER_SIZE = 4096;

      let ws = null,
        audioCtx = null,
        processor = null,
        analyser = null,
        stream = null,
        meterRaf = null,
        isMuted = false;

      const isSecureContext = window.isSecureContext;
      if (!isSecureContext) httpsWarn.classList.add("visible");

      function setStatus(text, cls) {
        badge.className = "badge " + (cls || "");
        badgeText.textContent = text;
      }

      function wsUrl() {
        const proto = location.protocol === "https:" ? "wss:" : "ws:";
        return `${proto}//${location.host}/ws`;
      }

      function applyMuteVisuals() {
        if (isMuted) {
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
          setStatus("Connected", "connected");
        }
      }

      async function startAudio() {
        if (!isSecureContext)
          throw new Error("Requires HTTPS. See warning above.");
        if (!navigator.mediaDevices?.getUserMedia)
          throw new Error("getUserMedia not available.");

        stream = await navigator.mediaDevices.getUserMedia({
          audio: {
            sampleRate: SAMPLE_RATE,
            channelCount: 1,
            echoCancellation: false,
            noiseSuppression: false,
            autoGainControl: false,
          },
        });

        audioCtx = new AudioContext({ sampleRate: SAMPLE_RATE });
        const src = audioCtx.createMediaStreamSource(stream);

        analyser = audioCtx.createAnalyser();
        analyser.fftSize = 64;
        analyser.smoothingTimeConstant = 0.7;

        processor = audioCtx.createScriptProcessor(BUFFER_SIZE, 1, 1);
        processor.onaudioprocess = (ev) => {
          if (!ws || ws.readyState !== WebSocket.OPEN) return;
          if (isMuted) return;
          const f32 = ev.inputBuffer.getChannelData(0);
          const i16 = new Int16Array(f32.length);
          for (let i = 0; i < f32.length; i++) {
            const c = Math.max(-1, Math.min(1, f32[i]));
            i16[i] = c < 0 ? c * 0x8000 : c * 0x7fff;
          }
          ws.send(i16.buffer);
        };

        src.connect(analyser);
        src.connect(processor);
        processor.connect(audioCtx.destination);

        startMeter();
      }

      function startMeter() {
        meterWrap.classList.add("active");
        barsEl.classList.add("active", "live");

        const timeBuf = new Uint8Array(analyser.frequencyBinCount);
        const freqBuf = new Uint8Array(analyser.frequencyBinCount);

        function tick() {
          analyser.getByteTimeDomainData(timeBuf);
          let peak = 0;
          for (let i = 0; i < timeBuf.length; i++)
            peak = Math.max(peak, Math.abs(timeBuf[i] - 128));
          meterFill.style.width = Math.min(100, (peak / 128) * 200) + "%";

          analyser.getByteFrequencyData(freqBuf);
          const step = Math.floor(freqBuf.length / barSpans.length);
          barSpans.forEach((bar, i) => {
            const val = isMuted ? 0 : freqBuf[i * step] || 0;
            bar.style.height = 4 + (val / 255) * 22 + "px";
          });

          meterRaf = requestAnimationFrame(tick);
        }
        meterRaf = requestAnimationFrame(tick);
      }

      function stopMeter() {
        if (meterRaf) cancelAnimationFrame(meterRaf);
        meterRaf = null;
        meterWrap.classList.remove("active");
        meterFill.style.width = "0%";
        barsEl.classList.remove("active", "live", "muted");
        barSpans.forEach((b) => (b.style.height = "4px"));
      }

      function connect() {
        if (ws) return;
        btn.disabled = true;
        setStatus("Connecting…", "connecting");

        ws = new WebSocket(wsUrl());
        ws.binaryType = "arraybuffer";

        ws.onopen = async () => {
          try {
            await startAudio();
            setStatus("Connected", "connected");
            btn.textContent = "Disconnect";
            btn.classList.add("disconnect");
            btn.disabled = false;
            muteBtn.classList.add("visible");
          } catch (err) {
            setStatus("Error: " + err.message, "error");
            ws.close();
          }
        };

        ws.onmessage = ({ data }) => {
          if (typeof data === "string") console.info("Server:", data);
        };

        ws.onclose = () => {
          cleanup();
          setStatus("Disconnected", "");
          btn.textContent = "Connect";
          btn.classList.remove("disconnect");
          btn.disabled = false;
          muteBtn.classList.remove("visible", "muted");
          isMuted = false;
        };

        ws.onerror = () => {
          setStatus("Connection error", "error");
        };
      }

      function disconnect() {
        ws?.close();
        cleanup();
      }

      function cleanup() {
        isMuted = false;
        stopMeter();
        processor?.disconnect();
        processor = null;
        audioCtx?.close();
        audioCtx = null;
        analyser = null;
        stream?.getTracks().forEach((t) => t.stop());
        stream = null;
        ws = null;
      }

      muteBtn.addEventListener("click", () => {
        isMuted = !isMuted;
        applyMuteVisuals();
      });

      btn.addEventListener("click", () => {
        if (!ws || ws.readyState !== WebSocket.OPEN) connect();
        else disconnect();
      });
    </script>
  </body>
</html>
"#;
