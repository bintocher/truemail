// Логика окна собственных уведомлений truemail.
(function () {
  const tauri = window.__TAURI__;
  if (!tauri) return;
  const invoke = tauri.core.invoke;
  const listen = tauri.event.listen;
  const stack = document.getElementById("stack");
  const MAX_CARDS = 4;
  const AUTO_CLOSE_MS = 12000;

  function escapeHtml(value) {
    return String(value == null ? "" : value)
      .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  function remaining() {
    return stack.querySelectorAll(".card:not(.leaving)").length;
  }

  // Окно всегда ровно по высоте карточек - иначе прозрачный остаток
  // перехватывает клики по главному окну.
  function syncSize() {
    const height = Math.ceil(stack.getBoundingClientRect().height);
    invoke("notify_resize", { height: Math.max(height, 1) }).catch(() => {});
  }

  function dismiss(card) {
    if (card.dataset.leaving) return;
    card.dataset.leaving = "1";
    card.classList.add("leaving");
    clearTimeout(card._timer);
    setTimeout(() => {
      card.remove();
      // Когда карточек не осталось - прячем окно.
      const hasMore = remaining() > 0;
      if (hasMore) syncSize();
      invoke("notify_close", { hasMore }).catch(() => {});
    }, 190);
  }

  function addCard(data) {
    const isEvent = data.kind === "event";
    const card = document.createElement("div");
    card.className = "card";
    const brandName = isEvent ? "Напоминание" : "truemail";
    const icon = isEvent ? "◷" : "✉";
    card.innerHTML =
      `<div class="head"><div class="brand"><span class="dot">${icon}</span><span>${escapeHtml(brandName)}</span></div>` +
      `<button class="close-x" title="Закрыть">×</button></div>` +
      `<div class="title"></div><div class="subject"></div>` +
      (data.preview ? `<div class="preview"></div>` : "") +
      `<div class="actions"></div>`;
    card.querySelector(".title").textContent = data.title || "";
    card.querySelector(".subject").textContent = data.subject || "";
    if (data.preview) card.querySelector(".preview").textContent = data.preview;

    const actions = card.querySelector(".actions");
    if (isEvent) {
      const urls = Array.isArray(data.urls) ? data.urls : [];
      urls.forEach((url, i) => {
        const label = urls.length === 1 ? "Присоединиться" : linkLabel(url);
        const b = mkBtn(label, i === 0, () => { invoke("notify_open_url", { url }).catch(() => {}); });
        b.title = url;
        actions.appendChild(b);
      });
      const open = mkBtn("Открыть", urls.length === 0, () => { invoke("notify_open", { messageId: null }).catch(() => {}); dismiss(card); });
      actions.appendChild(open);
    } else {
      const open = mkBtn("Открыть", true, () => { invoke("notify_open", { messageId: data.message_id ?? null }).catch(() => {}); dismiss(card); });
      const read = mkBtn("Прочитано", false, () => {
        if (data.message_id != null) invoke("mark_seen", { messageId: data.message_id, seen: true }).catch(() => {});
        dismiss(card);
      });
      actions.append(open, read);
    }
    const close = mkBtn("Закрыть", false, () => dismiss(card));
    actions.appendChild(close);
    card.querySelector(".close-x").onclick = () => dismiss(card);

    stack.appendChild(card);
    while (stack.querySelectorAll(".card").length > MAX_CARDS) {
      stack.firstElementChild.remove();
    }
    card._timer = setTimeout(() => dismiss(card), AUTO_CLOSE_MS);
    syncSize();
  }

  function linkLabel(url) {
    try { return new URL(url).hostname.replace(/^www\./, ""); }
    catch { return "Ссылка"; }
  }

  function mkBtn(label, primary, onClick) {
    const b = document.createElement("button");
    b.className = "btn" + (primary ? " primary" : "");
    b.textContent = label;
    b.onclick = onClick;
    return b;
  }

  listen("notify-push", (event) => {
    try { addCard(event.payload || {}); } catch (e) { console.error(e); }
  });
})();
