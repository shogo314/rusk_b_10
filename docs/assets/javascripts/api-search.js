(() => {
  function initialize() {
    const form = document.querySelector("[data-api-search]");
    if (!form || form.dataset.ready) return;
    form.dataset.ready = "true";

    const input = form.querySelector("input");
    const status = document.getElementById("rusk-api-status");
    const entries = Array.from(document.querySelectorAll(".rusk-api-entry"));
    const files = Array.from(document.querySelectorAll(".rusk-api-file"));
    const normalize = (text) => text.normalize("NFKC").toLowerCase();
    const index = entries.map((element) => ({
      element,
      text: normalize(element.textContent),
    }));
    const tocLinks = Array.from(document.querySelectorAll(".md-nav--secondary a[href^='#']"));

    function filter() {
      const terms = normalize(input.value).trim().split(/\s+/).filter(Boolean);
      let count = 0;
      for (const entry of index) {
        entry.element.hidden = !terms.every((term) => entry.text.includes(term));
        if (!entry.element.hidden) count += 1;
      }
      for (const file of files) {
        file.hidden = terms.length > 0 && !Array.from(file.querySelectorAll(".rusk-api-entry"))
          .some((entry) => !entry.hidden);
      }
      for (const link of tocLinks) {
        const target = document.getElementById(decodeURIComponent(link.hash.slice(1)));
        const section = target?.closest(".rusk-api-entry, .rusk-api-file");
        const item = link.closest("li");
        if (item && section) item.hidden = section.hidden;
      }
      status.textContent = count === 0
        ? "一致するAPIがありません。検索語を変えてください。"
        : `${count} / ${entries.length} 件`;
    }

    function saveQuery(clearAnchor = false) {
      const url = new URL(window.location.href);
      if (clearAnchor) url.hash = "";
      if (input.value.trim()) url.searchParams.set("api", input.value);
      else url.searchParams.delete("api");
      window.history.replaceState(window.history.state, "", url);
    }

    function revealAnchor() {
      let id;
      try {
        id = decodeURIComponent(window.location.hash.slice(1));
      } catch {
        return;
      }
      const target = document.getElementById(id);
      const section = target?.closest(".rusk-api-entry, .rusk-api-file");
      if (section?.hidden) {
        input.value = "";
        filter();
        saveQuery();
        target.scrollIntoView();
      }
    }

    input.value = new URL(window.location.href).searchParams.get("api") || "";
    input.addEventListener("input", () => {
      filter();
      saveQuery(true);
    });
    form.addEventListener("submit", (event) => event.preventDefault());
    form.addEventListener("reset", (event) => {
      event.preventDefault();
      input.value = "";
      filter();
      saveQuery(true);
      input.focus();
    });
    window.addEventListener("hashchange", revealAnchor);
    window.addEventListener("popstate", () => {
      input.value = new URL(window.location.href).searchParams.get("api") || "";
      filter();
      revealAnchor();
    });
    form.hidden = false;
    filter();
    revealAnchor();
  }

  if (typeof document$ !== "undefined") document$.subscribe(initialize);
  else if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", initialize);
  else initialize();
})();
