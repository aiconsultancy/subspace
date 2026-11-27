// The Space owns this stylesheet. AgentsView still owns its appearance preferences.
(({ origin, css }) => {
  if (window.location.origin !== origin) return;

  const install = () => {
    const id = "subspace-agentsview-appearance";
    let style = document.getElementById(id);
    if (!style && !css) return;

    const respectLightMode = () => {
      try {
        style.disabled = window.localStorage.getItem("theme") === "light";
      } catch {
        // Private views can block storage. The stylesheet still applies.
        style.disabled = false;
      }
    };
    if (!style) {
      style = document.createElement("style");
      style.id = id;
      document.head.append(style);
      new MutationObserver(respectLightMode).observe(document.documentElement, {
        attributes: true,
        attributeFilter: ["class"],
      });
    }
    style.textContent = css ?? "";
    respectLightMode();
  };

  if (document.head) install();
  else document.addEventListener("DOMContentLoaded", install, { once: true });
})(/*__SUBSPACE_AGENTSVIEW_APPEARANCE__*/);
