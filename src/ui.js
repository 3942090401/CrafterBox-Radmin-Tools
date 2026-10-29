let modalRoot = null;

function ensureRoot() {
  if (!modalRoot) {
    modalRoot = document.createElement("div");
    modalRoot.id = "__modal_root";
    document.body.appendChild(modalRoot);
  }
  return modalRoot;
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

export function confirmDialog(message, title) {
  if (!title) title = "\u786E\u8BA4";
  return new Promise((resolve) => {
    const root = ensureRoot();
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";

    const lines = message.split("\n").map(l => "<div>" + escapeHtml(l) + "</div>").join("");

    overlay.innerHTML =
      '<div class="modal-box">' +
        '<div class="modal-title">' + escapeHtml(title) + '</div>' +
        '<div class="modal-body">' + lines + '</div>' +
        '<div class="modal-actions">' +
          '<button class="modal-btn modal-cancel">\u53D6\u6D88</button>' +
          '<button class="modal-btn modal-ok">\u786E\u5B9A</button>' +
        '</div>' +
      '</div>';

    root.appendChild(overlay);
    requestAnimationFrame(() => overlay.classList.add("show"));

    const cleanup = (result) => {
      overlay.classList.remove("show");
      setTimeout(() => {
        overlay.remove();
        resolve(result);
      }, 200);
    };

    overlay.querySelector(".modal-ok").onclick = () => cleanup(true);
    overlay.querySelector(".modal-cancel").onclick = () => cleanup(false);
    overlay.onclick = (e) => { if (e.target === overlay) cleanup(false); };

    const onKey = (e) => {
      if (e.key === "Escape") {
        document.removeEventListener("keydown", onKey);
        cleanup(false);
      } else if (e.key === "Enter") {
        document.removeEventListener("keydown", onKey);
        cleanup(true);
      }
    };
    document.addEventListener("keydown", onKey);
  });
}

export function alertDialog(message, title, options) {
  if (!title) title = "\u63D0\u793A";
  if (!options) options = {};
  return new Promise((resolve) => {
    const root = ensureRoot();
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";

    const type = options.type || "info";
    const icons = {
      success: '<svg class="modal-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 4 4L19 6" /></svg>',
      error: '<svg class="modal-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12" /></svg>',
      info: '<svg class="modal-icon" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 11v5m0-8h.01" /></svg>'
    };
    const icon = icons[type] || icons.info;
    const lines = message.split("\n").map(l => "<div>" + escapeHtml(l) + "</div>").join("");

    overlay.innerHTML =
      '<div class="modal-box modal-' + escapeHtml(type) + '">' +
        '<div class="modal-title">' + icon + ' ' + escapeHtml(title) + '</div>' +
        '<div class="modal-body">' + lines + '</div>' +
        '<div class="modal-actions">' +
          (options.copyable ? '<button class="modal-btn modal-copy">\u590D\u5236</button>' : '') +
          '<button class="modal-btn modal-ok">\u786E\u5B9A</button>' +
        '</div>' +
      '</div>';

    root.appendChild(overlay);
    requestAnimationFrame(() => overlay.classList.add("show"));

    const cleanup = () => {
      overlay.classList.remove("show");
      setTimeout(() => {
        overlay.remove();
        resolve();
      }, 200);
    };

    overlay.querySelector(".modal-ok").onclick = cleanup;
    overlay.onclick = (e) => { if (e.target === overlay) cleanup(); };

    const copyBtn = overlay.querySelector(".modal-copy");
    if (copyBtn) {
      copyBtn.onclick = async () => {
        try {
          await navigator.clipboard.writeText(message);
          copyBtn.textContent = "\u5DF2\u590D\u5236";
          setTimeout(() => { copyBtn.textContent = "\u590D\u5236"; }, 1000);
        } catch (e) {}
      };
    }

    const onKey = (e) => {
      if (e.key === "Escape" || e.key === "Enter") {
        document.removeEventListener("keydown", onKey);
        cleanup();
      }
    };
    document.addEventListener("keydown", onKey);
  });
}

export function promptDialog(message, defaultValue, title) {
  if (defaultValue == null) defaultValue = "";
  if (!title) title = "\u8F93\u5165";
  return new Promise((resolve) => {
    const root = ensureRoot();
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";

    overlay.innerHTML =
      '<div class="modal-box">' +
        '<div class="modal-title">' + escapeHtml(title) + '</div>' +
        '<div class="modal-body">' +
          '<div>' + escapeHtml(message) + '</div>' +
          '<input class="modal-input" type="text" value="' + escapeHtml(defaultValue) + '" />' +
        '</div>' +
        '<div class="modal-actions">' +
          '<button class="modal-btn modal-cancel">\u53D6\u6D88</button>' +
          '<button class="modal-btn modal-ok">\u786E\u5B9A</button>' +
        '</div>' +
      '</div>';

    root.appendChild(overlay);
    requestAnimationFrame(() => overlay.classList.add("show"));

    const input = overlay.querySelector(".modal-input");
    input.focus();
    input.select();

    const cleanup = (result) => {
      overlay.classList.remove("show");
      setTimeout(() => {
        overlay.remove();
        resolve(result);
      }, 200);
    };

    overlay.querySelector(".modal-ok").onclick = () => cleanup(input.value);
    overlay.querySelector(".modal-cancel").onclick = () => cleanup(null);
    overlay.onclick = (e) => { if (e.target === overlay) cleanup(null); };

    const onKey = (e) => {
      if (e.key === "Escape") {
        document.removeEventListener("keydown", onKey);
        cleanup(null);
      } else if (e.key === "Enter") {
        document.removeEventListener("keydown", onKey);
        cleanup(input.value);
      }
    };
    document.addEventListener("keydown", onKey);
  });
}

export function toast(message, type = "success", duration = 2400) {
  let root = document.getElementById("__toast_root");
  if (!root) {
    root = document.createElement("div");
    root.id = "__toast_root";
    root.className = "toast-root";
    document.body.appendChild(root);
  }
  const icons = {
    success: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 4 4L19 6" /></svg>',
    error: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12" /></svg>',
    info: '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 11v5m0-8h.01" /></svg>'
  };
  const item = document.createElement("div");
  item.className = `app-toast toast-${icons[type] ? type : "info"}`;
  item.innerHTML = `${icons[type] || icons.info}<span>${escapeHtml(message)}</span>`;
  root.appendChild(item);
  requestAnimationFrame(() => item.classList.add("show"));
  setTimeout(() => {
    item.classList.remove("show");
    setTimeout(() => item.remove(), 200);
  }, duration);
}