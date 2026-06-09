// Build-order trainer — progress tracking, focus mode, DE game-clock timer (1.7×),
// copy-as-text, print. Vanilla DOM + localStorage (ThemeToggle pattern), zero deps.
// Re-initialised on every astro:page-load so it survives ClientRouter navigations.

function initTrainer(root: HTMLElement): void {
  if (root.dataset.trainerReady === "1") return;
  root.dataset.trainerReady = "1";

  const slug = root.dataset.slug ?? "build";
  const steps = Array.from(root.querySelectorAll<HTMLElement>("[data-step]"));
  if (steps.length === 0) return;
  const KEY = `bo:${slug}`;
  const $ = (sel: string) => root.querySelector<HTMLElement>(sel);

  // ---- progress (persisted) ----
  let done = new Set<number>();
  try {
    done = new Set(JSON.parse(localStorage.getItem(KEY) ?? "[]") as number[]);
  } catch {
    /* private mode / bad JSON — start empty */
  }
  const save = () => {
    try {
      localStorage.setItem(KEY, JSON.stringify([...done]));
    } catch {
      /* ignore */
    }
  };
  const progressEl = $("[data-progress]");
  const renderProgress = () => {
    if (progressEl) progressEl.textContent = `${done.size}/${steps.length}`;
  };
  const applyDone = () => {
    steps.forEach((s, i) => {
      s.classList.toggle("is-done", done.has(i));
    });
  };
  const toggleDone = (i: number) => {
    if (done.has(i)) done.delete(i);
    else done.add(i);
    save();
    applyDone();
    renderProgress();
  };

  // ---- current step (keyboard nav + focus mode) ----
  let current = 0;
  const applyCurrent = (scroll = false) => {
    steps.forEach((s, i) => {
      s.classList.toggle("is-current", i === current);
    });
    if (scroll) steps[current]?.scrollIntoView({ block: "center", behavior: "smooth" });
  };
  const move = (delta: number) => {
    current = Math.max(0, Math.min(steps.length - 1, current + delta));
    applyCurrent(true);
  };

  steps.forEach((s, i) => {
    s.addEventListener("click", () => {
      current = i;
      toggleDone(i);
      applyCurrent();
    });
  });
  root.addEventListener("keydown", (e) => {
    if (e.key === "j" || e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "k" || e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      toggleDone(current);
    }
  });

  // ---- toolbar ----
  $("[data-action=reset-progress]")?.addEventListener("click", () => {
    done.clear();
    save();
    applyDone();
    renderProgress();
  });
  $("[data-action=focus]")?.addEventListener("click", () => {
    root.classList.toggle("focus-mode");
    applyCurrent(true);
  });
  $("[data-action=print]")?.addEventListener("click", () => window.print());

  const copyBtn = $("[data-action=copy]");
  copyBtn?.addEventListener("click", async () => {
    const lines = steps.map((s) => {
      const time = s.dataset.time ? `[${s.dataset.time}] ` : "";
      const pop = s.dataset.pop ? `${s.dataset.pop} vil — ` : "";
      return `${time}${pop}${s.dataset.assign ?? ""}`;
    });
    try {
      await navigator.clipboard.writeText(`${slug}\n${lines.join("\n")}`);
      const original = copyBtn.textContent;
      copyBtn.textContent = copyBtn.dataset.copied ?? "Copied!";
      setTimeout(() => {
        copyBtn.textContent = original;
      }, 1500);
    } catch {
      /* clipboard blocked */
    }
  });

  // ---- game-clock timer (in-game time = wall × speed; DE Normal = 1.7×) ----
  const clockEl = $("[data-clock]");
  const startBtn = $("[data-action=timer]");
  const speedSel = root.querySelector<HTMLSelectElement>("[data-speed]");
  const parseTime = (t?: string): number | null => {
    const m = t?.match(/(\d+):(\d+)/);
    return m ? Number(m[1]) * 60 + Number(m[2]) : null;
  };
  const stepSecs = steps.map((s) => parseTime(s.dataset.time || undefined));
  let running = false;
  let gameMs = 0;
  let lastTs = 0;
  let speed = 1.7;
  const fmt = (sec: number) =>
    `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, "0")}`;
  const tick = (ts: number) => {
    if (!running) return;
    if (lastTs) gameMs += (ts - lastTs) * speed;
    lastTs = ts;
    const sec = gameMs / 1000;
    if (clockEl) clockEl.textContent = fmt(sec);
    steps.forEach((s, i) => {
      const t = stepSecs[i];
      if (t != null) s.classList.toggle("is-due", sec >= t);
    });
    requestAnimationFrame(tick);
  };
  startBtn?.addEventListener("click", () => {
    running = !running;
    lastTs = 0;
    startBtn.textContent = running
      ? (startBtn.dataset.pause ?? "Pause")
      : (startBtn.dataset.start ?? "Start");
    if (running) requestAnimationFrame(tick);
  });
  $("[data-action=timer-reset]")?.addEventListener("click", () => {
    running = false;
    gameMs = 0;
    lastTs = 0;
    if (clockEl) clockEl.textContent = "0:00";
    if (startBtn) startBtn.textContent = startBtn.dataset.start ?? "Start";
    steps.forEach((s) => {
      s.classList.remove("is-due");
    });
  });
  speedSel?.addEventListener("change", () => {
    speed = Number.parseFloat(speedSel.value) || 1.7;
  });

  applyDone();
  applyCurrent();
  renderProgress();
}

const initTrainers = () =>
  document.querySelectorAll<HTMLElement>("[data-trainer]").forEach(initTrainer);
document.addEventListener("astro:page-load", initTrainers);

export {}; // mark as a module so top-level names don't collide with other scripts
