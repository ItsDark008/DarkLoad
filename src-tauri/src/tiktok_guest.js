(() => {
  if (window.top !== window || location.origin !== "https://www.tiktok.com" ||
      !location.pathname.startsWith("/player/v1/")) return;

  let reported = false;
  const isItemsResponse = (url) => {
    try {
      const parsed = new URL(url);
      return parsed.origin === location.origin && parsed.pathname === "/player/api/v1/items";
    } catch {
      return false;
    }
  };

  const report = (data) => {
    if (reported) return;
    const message = JSON.stringify({ data, userAgent: navigator.userAgent });
    if (!message || message.length > 128_000) return;
    reported = true;
    // Leave time for the player's initial media requests to set the guest cookies.
    setTimeout(() => location.replace("DARKLOAD_GUEST_REPORT_URL#data=" + encodeURIComponent(message)), 750);
  };

  const send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function (...args) {
    this.addEventListener("load", () => {
      if (this.status !== 200 || !isItemsResponse(this.responseURL)) return;
      try {
        report(this.responseType === "json" ? this.response : JSON.parse(this.responseText));
      } catch {
        // The backend's timeout handles a player that returned no usable data.
      }
    }, { once: true });
    return Reflect.apply(send, this, args);
  };

  const fetch = window.fetch;
  window.fetch = async function (...args) {
    const response = await Reflect.apply(fetch, this, args);
    if (response.ok && isItemsResponse(response.url)) {
      response.clone().json().then(report).catch(() => {});
    }
    return response;
  };
})();
