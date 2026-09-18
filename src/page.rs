pub const HTML: &str = r#"
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Remote Microphone</title>
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link
      href="https://fonts.googleapis.com/css2?family=DM+Mono:wght@300;400;500&family=Instrument+Serif&display=swap"
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
        <h2>Use this device as your computer’s microphone.</h2>
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
          <div class="fact">
            <span class="fact-label">RTT</span>
            <span class="fact-value" id="rtt-state">—</span>
          </div>
          <div class="fact">
            <span class="fact-label">Network</span>
            <span class="fact-value" id="network-state">—</span>
          </div>
          <div class="fact">
            <span class="fact-label">Queue</span>
            <span class="fact-value" id="queue-state">—</span>
          </div>
        </div>
      </div>

      <div id="https-warning" class="warn-box">
        <strong>A trusted secure link is needed</strong><br />
        Install <a href="/remotemic-ca.crt">RemoteMic Local CA</a> as a trusted
        root according to this device’s certificate settings, then reopen the
        local HTTPS address.
      </div>

      <div id="wake-warning" class="warn-box">
        <strong>Keep the screen on</strong><br />
        This browser cannot prevent sleep. Leave this page visible and do not
        lock the device while streaming.
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
          <span id="mute-label">Mute</span>
        </button>
      </div>

      <p class="hint">
        Your microphone is used only while this page is connected.
      </p>
    </div>

    <script>
      "use strict";

      const TOKEN = "__REMOTEMIC_TOKEN__";
      const OPUS_BITRATE = __REMOTEMIC_OPUS_BITRATE__;
      const btn = document.getElementById("btn");
      const muteBtn = document.getElementById("mute-btn");
      const muteLabel = document.getElementById("mute-label");
      const badge = document.getElementById("badge");
      const badgeText = document.getElementById("badge-text");
      const micState = document.getElementById("mic-state");
      const streamState = document.getElementById("stream-state");
      const wakeState = document.getElementById("wake-state");
      const rttState = document.getElementById("rtt-state");
      const networkState = document.getElementById("network-state");
      const queueState = document.getElementById("queue-state");
      const httpsWarning = document.getElementById("https-warning");
      const wakeWarning = document.getElementById("wake-warning");
      const bars = document.getElementById("bars");
      const barItems = [...bars.querySelectorAll("span")];
      const meterWrap = document.getElementById("meter-wrap");
      const meterFill = document.getElementById("meter-fill");

      let session = null;

      function setBadge(text, state = "") {
        badge.className = "badge" + (state ? " " + state : "");
        badgeText.textContent = text;
      }

      function websocketUrl() {
        const protocol = location.protocol === "https:" ? "wss:" : "ws:";
        return protocol + "//" + location.host + "/ws?token=" + encodeURIComponent(TOKEN);
      }

      function waitForSocket(socket) {
        return new Promise((resolve, reject) => {
          socket.addEventListener("open", resolve, { once: true });
          socket.addEventListener(
            "error",
            () => reject(new Error("Signaling connection failed")),
            { once: true },
          );
        });
      }

      function waitForIceGathering(peer) {
        if (peer.iceGatheringState === "complete") return Promise.resolve();
        return new Promise((resolve, reject) => {
          const timeout = setTimeout(
            () => reject(new Error("ICE gathering timed out")),
            10000,
          );
          function changed() {
            if (peer.iceGatheringState !== "complete") return;
            clearTimeout(timeout);
            peer.removeEventListener("icegatheringstatechange", changed);
            resolve();
          }
          peer.addEventListener("icegatheringstatechange", changed);
        });
      }

      function preferOpus(peer) {
        const transceiver = peer
          .getTransceivers()
          .find((item) => item.sender?.track?.kind === "audio");
        const codecs = RTCRtpSender.getCapabilities?.("audio")?.codecs || [];
        const opus = codecs.filter(
          (codec) => codec.mimeType.toLowerCase() === "audio/opus",
        );
        if (opus.length && transceiver?.setCodecPreferences) {
          transceiver.setCodecPreferences(opus);
        }
      }

      function tuneOffer(offer) {
        if (/a=ptime:\d+\r?\n/i.test(offer.sdp)) {
          offer.sdp = offer.sdp.replace(/a=ptime:\d+/i, "a=ptime:10");
        } else {
          offer.sdp = offer.sdp.replace(
            /(m=audio[^\r\n]*\r?\n)/i,
            "$1a=ptime:10\r\n",
          );
        }
        offer.sdp = offer.sdp.replace(
          /a=fmtp:(\d+) ([^\r\n]*useinbandfec=1[^\r\n]*)/i,
          (_line, payloadType, fmtp) =>
            "a=fmtp:" +
            payloadType +
            " " +
            fmtp +
            ";stereo=0;sprop-stereo=0;usedtx=0;maxaveragebitrate=" +
            OPUS_BITRATE,
        );
        return offer;
      }

      async function tuneSender(sender) {
        const parameters = sender.getParameters();
        if (!parameters.encodings?.length) parameters.encodings = [{}];
        parameters.encodings[0].maxBitrate = OPUS_BITRATE;
        parameters.encodings[0].priority = "high";
        parameters.encodings[0].networkPriority = "high";
        try {
          await sender.setParameters(parameters);
        } catch (error) {
          console.debug("Sender tuning is not supported", error);
        }
      }

      async function acquireWakeLock(activeSession) {
        if (!("wakeLock" in navigator)) {
          wakeState.textContent = "Unavailable";
          wakeWarning.classList.add("visible");
          return;
        }
        try {
          activeSession.wakeLock = await navigator.wakeLock.request("screen");
          wakeState.textContent = "Kept awake";
          activeSession.wakeLock.addEventListener("release", () => {
            if (session === activeSession && !activeSession.stopping) {
              wakeState.textContent = "Released";
            }
          });
        } catch (error) {
          console.debug("Wake lock unavailable", error);
          wakeState.textContent = "Unavailable";
          wakeWarning.classList.add("visible");
        }
      }

      function startMeter(activeSession) {
        const analyser = activeSession.analyser;
        const values = new Uint8Array(analyser.frequencyBinCount);
        function draw() {
          if (session !== activeSession || activeSession.stopping) return;
          analyser.getByteFrequencyData(values);
          let peak = 0;
          for (const value of values) peak = Math.max(peak, value);
          const level = Math.min(100, (peak / 255) * 135);
          meterFill.style.width = level + "%";
          for (let index = 0; index < barItems.length; index += 1) {
            const sourceIndex = Math.floor(
              (index / barItems.length) * Math.min(values.length, 48),
            );
            barItems[index].style.height =
              Math.max(4, (values[sourceIndex] / 255) * 28) + "px";
          }
          activeSession.animationFrame = requestAnimationFrame(draw);
        }
        bars.classList.add("active", "live");
        meterWrap.classList.add("active");
        draw();
      }

      async function updateMetrics(activeSession) {
        if (session !== activeSession || activeSession.stopping) return;
        try {
          const [stats, response] = await Promise.all([
            activeSession.peer.getStats(),
            fetch("/metrics", { cache: "no-store" }),
          ]);
          stats.forEach((report) => {
            if (
              report.type === "candidate-pair" &&
              report.state === "succeeded" &&
              report.currentRoundTripTime != null
            ) {
              rttState.textContent =
                (report.currentRoundTripTime * 1000).toFixed(1) + " ms";
            }
            if (
              report.type === "remote-inbound-rtp" &&
              report.kind === "audio" &&
              report.roundTripTime != null
            ) {
              rttState.textContent =
                (report.roundTripTime * 1000).toFixed(1) + " ms";
            }
          });
          if (response.ok) {
            const metrics = await response.json();
            networkState.textContent =
              metrics.jitterMs.toFixed(1) +
              " ms jitter · " +
              metrics.lossPercent.toFixed(2) +
              "% loss";
            queueState.textContent = metrics.queueMs.toFixed(1) + " ms";
          }
        } catch (error) {
          console.debug("Metrics update failed", error);
        }
      }

      async function connect() {
        if (!window.isSecureContext) {
          throw new Error("HTTPS and a trusted local certificate are required");
        }

        btn.disabled = true;
        setBadge("Requesting access", "connecting");
        micState.textContent = "Waiting for permission";
        wakeWarning.classList.remove("visible");

        const stream = await navigator.mediaDevices.getUserMedia({
          audio: {
            sampleRate: { ideal: 48000 },
            channelCount: { exact: 1 },
            echoCancellation: false,
            noiseSuppression: false,
            autoGainControl: false,
            latency: { ideal: 0.01 },
          },
          video: false,
        });
        const track = stream.getAudioTracks()[0];
        if ("contentHint" in track) track.contentHint = "music";

        const AudioContextClass =
          window.AudioContext || window.webkitAudioContext;
        const audioContext = new AudioContextClass({ sampleRate: 48000 });
        await audioContext.resume();
        const source = audioContext.createMediaStreamSource(stream);
        const analyser = audioContext.createAnalyser();
        analyser.fftSize = 256;
        analyser.smoothingTimeConstant = 0.72;
        const silentGain = audioContext.createGain();
        silentGain.gain.value = 0;
        source.connect(analyser);
        analyser.connect(silentGain);
        silentGain.connect(audioContext.destination);

        const peer = new RTCPeerConnection({
          iceServers: [],
          bundlePolicy: "max-bundle",
        });
        const sender = peer.addTrack(track, stream);
        preferOpus(peer);
        await tuneSender(sender);

        const socket = new WebSocket(websocketUrl());
        const activeSession = {
          stream,
          track,
          audioContext,
          source,
          analyser,
          silentGain,
          peer,
          socket,
          wakeLock: null,
          metricsTimer: null,
          animationFrame: null,
          stopping: false,
          muted: false,
        };
        session = activeSession;

        socket.addEventListener("message", async ({ data }) => {
          if (session !== activeSession) return;
          try {
            const message = JSON.parse(data);
            if (message.type === "answer") {
              await peer.setRemoteDescription(message);
            } else if (message.type === "error") {
              stop(message.message, true);
            }
          } catch (error) {
            stop("Invalid signaling response: " + error.message, true);
          }
        });
        socket.addEventListener("close", () => {
          if (session === activeSession && !activeSession.stopping) {
            stop("Signaling connection closed", true);
          }
        });
        peer.addEventListener("connectionstatechange", () => {
          if (session !== activeSession) return;
          if (peer.connectionState === "connected") {
            setBadge("Connected", "connected");
            streamState.textContent = "WebRTC · Opus · LAN";
            btn.disabled = false;
          } else if (
            ["failed", "closed"].includes(peer.connectionState) &&
            !activeSession.stopping
          ) {
            stop("WebRTC " + peer.connectionState, true);
          }
        });

        await waitForSocket(socket);
        const offer = tuneOffer(await peer.createOffer());
        await peer.setLocalDescription(offer);
        await waitForIceGathering(peer);
        socket.send(JSON.stringify(peer.localDescription));

        const settings = track.getSettings();
        micState.textContent =
          (settings.sampleRate || 48000) + " Hz · mono · processing off";
        streamState.textContent = "Negotiating WebRTC";
        btn.textContent = "Disconnect";
        btn.classList.add("disconnect");
        btn.disabled = false;
        muteBtn.classList.add("visible");
        setBadge("Connecting", "connecting");
        startMeter(activeSession);
        await acquireWakeLock(activeSession);
        activeSession.metricsTimer = setInterval(
          () => updateMetrics(activeSession),
          1000,
        );
      }

      async function stop(reason = "Ready", failed = false) {
        const activeSession = session;
        if (!activeSession) {
          setBadge(reason, failed ? "error" : "");
          return;
        }
        activeSession.stopping = true;
        session = null;
        clearInterval(activeSession.metricsTimer);
        cancelAnimationFrame(activeSession.animationFrame);
        activeSession.stream.getTracks().forEach((track) => track.stop());
        activeSession.peer.close();
        activeSession.socket.close();
        activeSession.source.disconnect();
        activeSession.analyser.disconnect();
        activeSession.silentGain.disconnect();
        await activeSession.audioContext.close().catch(() => {});
        await activeSession.wakeLock?.release().catch(() => {});

        btn.disabled = false;
        btn.textContent = "Connect microphone";
        btn.classList.remove("disconnect");
        muteBtn.classList.remove("visible", "muted");
        muteBtn.setAttribute("aria-pressed", "false");
        muteLabel.textContent = "Mute";
        bars.classList.remove("active", "live", "muted");
        meterWrap.classList.remove("active");
        meterFill.classList.remove("muted");
        meterFill.style.width = "0%";
        barItems.forEach((bar) => {
          bar.style.height = "4px";
        });
        micState.textContent = "Off";
        streamState.textContent = "Not sending";
        wakeState.textContent = "Managed when connected";
        rttState.textContent = "—";
        networkState.textContent = "—";
        queueState.textContent = "—";
        setBadge(reason, failed ? "error" : "");
      }

      btn.addEventListener("click", async () => {
        if (session) {
          await stop();
          return;
        }
        try {
          await connect();
        } catch (error) {
          await stop(error.message || String(error), true);
        }
      });

      muteBtn.addEventListener("click", () => {
        if (!session) return;
        session.muted = !session.muted;
        session.track.enabled = !session.muted;
        muteBtn.classList.toggle("muted", session.muted);
        muteBtn.setAttribute("aria-pressed", String(session.muted));
        muteLabel.textContent = session.muted ? "Unmute" : "Mute";
        bars.classList.toggle("muted", session.muted);
        meterFill.classList.toggle("muted", session.muted);
        micState.textContent = session.muted ? "Muted" : "Active";
        setBadge(
          session.muted ? "Muted" : "Connected",
          session.muted ? "muted" : "connected",
        );
      });

      document.addEventListener("visibilitychange", () => {
        if (
          document.visibilityState === "visible" &&
          session &&
          !session.stopping &&
          !session.wakeLock
        ) {
          acquireWakeLock(session);
        }
      });

      window.addEventListener("pagehide", () => {
        if (session) stop();
      });

      if (!window.isSecureContext) {
        httpsWarning.classList.add("visible");
        btn.disabled = true;
        setBadge("HTTPS required", "error");
      }
    </script>
  </body>
</html>
"#;
