// Injected into DarkLoad's built-in YouTube browser. It adds a selection control to video
// thumbnails and reports picks to DarkLoad. The page has no IPC access: messages travel as a
// navigation to a sentinel URL that the backend reads and cancels, and the backend validates them.
(() => {
  if (window.top !== window) return;
  const host = location.hostname;
  if (host !== "youtube.com" && !host.endsWith(".youtube.com")) return;

  const REPORT = "DARKLOAD_BROWSER_REPORT_URL";
  // Interface text in the language DarkLoad uses, filled in by the backend.
  const TEXT = DARKLOAD_BROWSER_TEXT;
  const ID = /^[A-Za-z0-9_-]{11}$/;
  const MAX_SELECTION = 500;
  const CARD = [
    "ytd-rich-item-renderer",
    "ytd-video-renderer",
    "ytd-compact-video-renderer",
    "ytd-grid-video-renderer",
    "ytd-playlist-video-renderer",
    "ytd-playlist-panel-video-renderer",
    "ytd-reel-item-renderer",
    "ytd-movie-renderer",
    "yt-lockup-view-model",
    "ytm-shorts-lockup-view-model",
    "ytm-shorts-lockup-view-model-v2",
  ].join(",");
  const TITLE = ["#video-title", "h3 a[title]", "h3 [title]", "h3", "[class*='lockup-metadata'][class*='title']", "[class*='MetadataTitle']", "a[title]"];
  const UPLOADER = ["ytd-channel-name #text", "ytd-channel-name a", "#channel-name #text", "[class*='content-metadata'][class*='metadata-text']", "#byline"];
  const BADGE = ["ytd-thumbnail-overlay-time-status-renderer", "[class*='badge-shape'][class*='text']", "badge-shape", "#time-status"];
  // YouTube enforces Trusted Types, so markup is built with DOM calls rather than innerHTML.
  const SVG = "http://www.w3.org/2000/svg";
  const check = () => {
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", "16");
    svg.setAttribute("height", "16");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS(SVG, "path");
    for (const [name, value] of [["d", "M20 6 9 17l-5-5"], ["fill", "none"], ["stroke", "currentColor"], ["stroke-width", "3"], ["stroke-linecap", "round"], ["stroke-linejoin", "round"]]) {
      path.setAttribute(name, value);
    }
    svg.appendChild(path);
    return svg;
  };

  const selected = new Set();

  // ----- Messages to DarkLoad -------------------------------------------------------------------

  let outbox = [];
  let timer = 0;
  let last = 0;
  const flush = () => {
    timer = 0;
    if (outbox.length === 0) return;
    const batch = outbox.splice(0, 50);
    last = Date.now();
    location.replace(REPORT + "#data=" + encodeURIComponent(JSON.stringify(batch)));
    if (outbox.length) schedule();
  };
  // One report per short interval: a second navigation started at once would replace the first.
  const schedule = () => {
    if (!timer) timer = setTimeout(flush, Math.max(0, 80 - (Date.now() - last)));
  };
  const send = (message) => {
    if (message.t === "page") outbox = outbox.filter((m) => m.t !== "page");
    outbox.push(message);
    schedule();
  };

  // ----- Reading the page -----------------------------------------------------------------------

  const videoId = (href) => {
    try {
      const url = new URL(href, location.origin);
      if (url.hostname !== "youtube.com" && !url.hostname.endsWith(".youtube.com")) return null;
      let id = null;
      if (url.pathname === "/watch") id = url.searchParams.get("v");
      else id = (url.pathname.match(/^\/(?:shorts|live)\/([^/?#]+)/) || [])[1] || null;
      return id && ID.test(id) ? id : null;
    } catch {
      return null;
    }
  };

  const textOf = (el) => (el ? el.getAttribute("title") || el.getAttribute("aria-label") || el.textContent || "" : "").replace(/\s+/g, " ").trim();

  const first = (root, selectors, accept = (t) => t.length > 0) => {
    for (const selector of selectors) {
      for (const el of root.querySelectorAll(selector)) {
        const text = textOf(el);
        if (accept(text)) return text;
      }
    }
    return null;
  };

  const seconds = (text) => {
    const match = /(\d{1,2}:)?\d{1,2}:\d{2}/.exec(text || "");
    if (!match) return null;
    return match[0].split(":").reduce((total, part) => total * 60 + Number(part), 0) || null;
  };

  const describe = (card, anchor) => {
    const label = anchor.getAttribute("aria-label") || anchor.getAttribute("title");
    return {
      title: first(card, TITLE) || label || null,
      uploader: first(card, UPLOADER),
      duration: seconds(first(card, BADGE, (t) => /\d:\d\d/.test(t))),
    };
  };

  const currentVideo = () => {
    const id = videoId(location.href);
    if (!id) return null;
    const watch = document.querySelector("ytd-watch-metadata");
    const video = document.querySelector("video");
    const duration = video && Number.isFinite(video.duration) && video.duration > 0 ? video.duration : null;
    return {
      id,
      title: (watch && first(watch, ["h1 yt-formatted-string", "h1"])) || document.title.replace(/ - YouTube$/, "") || null,
      uploader: watch ? first(watch, ["ytd-channel-name a", "#channel-name a"]) : null,
      duration: location.pathname.startsWith("/watch") ? duration : null,
    };
  };

  const reportPage = () => send({ t: "page", url: location.href, title: document.title, video: currentVideo() });

  // ----- Selection --------------------------------------------------------------------------------

  const toggle = (id, info) => {
    const on = !selected.has(id);
    if (on && selected.size >= MAX_SELECTION) return;
    if (on) selected.add(id);
    else selected.delete(id);
    render();
    send({ t: "toggle", selected: on, video: { id, ...info } });
  };

  let pill = null;
  const render = () => {
    const root = document.documentElement;
    if (!root) return;
    root.classList.toggle("dl-selecting", selected.size > 0);
    for (const button of document.querySelectorAll(".dl-pick")) {
      const anchor = button.parentElement;
      const id = anchor ? videoId(anchor.href) : null;
      const on = id != null && selected.has(id);
      button.hidden = id == null;
      button.setAttribute("aria-pressed", String(on));
      button.title = on ? TEXT.deselect : TEXT.select;
      anchor?.classList.toggle("dl-on", on);
    }
    const current = videoId(location.href);
    if (pill) {
      pill.hidden = current == null;
      const on = current != null && selected.has(current);
      pill.setAttribute("aria-pressed", String(on));
      pill.lastChild.textContent = on ? TEXT.selected : TEXT.select;
    }
  };

  const decorate = () => {
    for (const card of document.querySelectorAll(CARD)) {
      let anchor = null;
      for (const a of card.querySelectorAll("a[href]")) {
        if (videoId(a.href) && a.querySelector("img, yt-image, yt-thumbnail-view-model")) {
          anchor = a;
          break;
        }
      }
      if (!anchor || anchor.querySelector(":scope > .dl-pick")) continue;
      card.classList.add("dl-card");
      anchor.classList.add("dl-host");
      // The control is placed against the thumbnail; leave YouTube's own positioning alone.
      if (getComputedStyle(anchor).position === "static") anchor.style.position = "relative";
      const button = document.createElement("button");
      button.type = "button";
      button.className = "dl-pick";
      button.appendChild(check());
      anchor.appendChild(button);
    }
    if (!pill && document.body) {
      pill = document.createElement("button");
      pill.type = "button";
      pill.className = "dl-pill";
      pill.append(check(), document.createElement("span"));
      document.body.appendChild(pill);
    }
    render();
  };

  // Clicks on DarkLoad's controls must never reach YouTube, which would open the video.
  const swallow = (event) => {
    const target = event.target instanceof Element ? event.target.closest(".dl-pick, .dl-pill") : null;
    if (!target) return;
    event.stopImmediatePropagation();
    event.preventDefault();
    if (event.type !== "click") return;
    if (target.classList.contains("dl-pill")) {
      const video = currentVideo();
      if (video) toggle(video.id, video);
      return;
    }
    const anchor = target.parentElement;
    const id = anchor && videoId(anchor.href);
    if (!id) return;
    const card = anchor.closest(".dl-card") || anchor;
    toggle(id, describe(card, anchor));
  };
  for (const type of ["click", "mousedown", "mouseup", "pointerdown", "pointerup", "touchstart", "touchend", "auxclick", "dblclick"]) {
    window.addEventListener(type, swallow, true);
  }

  Object.defineProperty(window, "__darkloadSync", {
    value: (ids) => {
      selected.clear();
      if (Array.isArray(ids)) for (const id of ids.slice(0, MAX_SELECTION)) if (typeof id === "string" && ID.test(id)) selected.add(id);
      render();
    },
  });

  const STYLE = `
    .dl-pick {
      position: absolute; top: 8px; left: 8px; z-index: 2147483000;
      width: 30px; height: 30px; padding: 0; margin: 0; border-radius: 8px; cursor: pointer;
      display: flex; align-items: center; justify-content: center;
      color: transparent; background: rgba(0, 0, 0, 0.62); border: 2px solid rgba(255, 255, 255, 0.9);
      box-shadow: 0 1px 6px rgba(0, 0, 0, 0.5); opacity: 0; transition: opacity 120ms, background 120ms, transform 120ms;
    }
    .dl-pick[hidden] { display: none; }
    .dl-card:hover .dl-pick, .dl-host:focus-within .dl-pick, html.dl-selecting .dl-pick, .dl-pick:focus-visible { opacity: 1; }
    .dl-pick:hover { transform: scale(1.06); background: rgba(0, 0, 0, 0.8); }
    .dl-pick[aria-pressed="true"] { opacity: 1; color: #fff; background: #8b5cf6; border-color: #8b5cf6; }
    .dl-pick:focus-visible, .dl-pill:focus-visible { outline: 2px solid #a78bfa; outline-offset: 2px; }
    .dl-host.dl-on::after {
      content: ""; position: absolute; inset: 0; z-index: 2147482999; pointer-events: none;
      border: 3px solid #8b5cf6; border-radius: 12px; background: rgba(139, 92, 246, 0.16);
    }
    /* Hover previews cover the thumbnail and its selection control. */
    ytd-video-preview, #video-preview { display: none !important; }
    .dl-pill {
      position: fixed; left: 16px; bottom: 16px; z-index: 2147483000;
      display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 16px 0 12px;
      border-radius: 20px; cursor: pointer; font: 500 14px/1 Roboto, Arial, sans-serif;
      color: #fff; background: rgba(15, 15, 15, 0.92); border: 1px solid rgba(255, 255, 255, 0.18);
      box-shadow: 0 4px 18px rgba(0, 0, 0, 0.5);
    }
    .dl-pill[hidden] { display: none; }
    .dl-pill svg { color: rgba(255, 255, 255, 0.35); }
    .dl-pill:hover { background: rgba(40, 40, 40, 0.95); }
    .dl-pill[aria-pressed="true"] { background: #8b5cf6; border-color: #8b5cf6; }
    .dl-pill[aria-pressed="true"] svg { color: #fff; }
  `;

  let queued = false;
  const refresh = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      decorate();
    });
  };

  const start = () => {
    const style = document.createElement("style");
    style.textContent = STYLE;
    (document.head || document.documentElement).appendChild(style);
    new MutationObserver(refresh).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ["href"] });
    decorate();
    reportPage();
  };

  // YouTube navigates in place; report each page once it is ready, and again once titles settle.
  let settle = 0;
  const onNavigate = () => {
    refresh();
    reportPage();
    clearTimeout(settle);
    settle = setTimeout(reportPage, 1500);
  };
  document.addEventListener("yt-navigate-finish", onNavigate);
  window.addEventListener("popstate", onNavigate);

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
  else start();
})();
