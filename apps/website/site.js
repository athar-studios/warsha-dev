(function () {
  "use strict";

  var H = document.documentElement;
  var motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  var R = motionQuery.matches;

  motionQuery.addEventListener("change", function (e) {
    R = e.matches;
  });

  // Language management
  var T = {
    ar: "عايزة تطبيق لعيادة الأسنان: المرضى يحجزوا من الموبايل، ويوصلهم تذكير قبل الميعاد بيوم.",
    en: "I need an app for my dental clinic: patients book from their phone and get a reminder a day before.",
  };

  var el = document.getElementById("typed");
  var ask = document.getElementById("ask");
  var typeTimer;

  function type() {
    if (!el) return;
    clearTimeout(typeTimer);
    if (ask) ask.classList.remove("on");
    var s = T[H.lang] || T.ar;
    var i = 0;

    if (R) {
      el.textContent = s;
      if (ask) ask.classList.add("on");
      return;
    }

    el.textContent = "";
    function tick() {
      el.textContent = s.slice(0, ++i);
      if (i < s.length) {
        typeTimer = setTimeout(tick, 28 + Math.random() * 35);
      } else if (ask) {
        typeTimer = setTimeout(function () {
          ask.classList.add("on");
        }, 400);
      }
    }
    tick();
  }

  function setLang(l) {
    H.lang = l;
    H.dir = l === "ar" ? "rtl" : "ltr";
    try {
      localStorage.setItem("warsha_lang", l);
    } catch (e) {
      // ignore storage errors
    }
    if (themeToggle) {
      var current = H.getAttribute("data-theme") === "light" ? "light" : "dark";
      themeToggle.setAttribute(
        "aria-label",
        current === "light"
          ? (l === "ar" ? "التبديل إلى الوضع الداكن" : "Switch to dark theme")
          : (l === "ar" ? "التبديل إلى الوضع الفاتح" : "Switch to light theme")
      );
    }
    type();
  }

  // Theme management (Dark / Light)
  var themeToggle = document.getElementById("themeToggle");
  var savedTheme = null;
  try {
    savedTheme = localStorage.getItem("warsha_theme");
  } catch (e) {
    // ignore storage errors
  }

  if (!savedTheme) {
    var prefersLight =
      window.matchMedia &&
      window.matchMedia("(prefers-color-scheme: light)").matches;
    savedTheme = prefersLight ? "light" : "dark";
  }

  function applyTheme(theme) {
    H.setAttribute("data-theme", theme);
    try {
      localStorage.setItem("warsha_theme", theme);
    } catch (e) {
      // ignore storage errors
    }
    if (themeToggle) {
      themeToggle.setAttribute(
        "aria-label",
        theme === "light"
          ? (H.lang === "ar" ? "التبديل إلى الوضع الداكن" : "Switch to dark theme")
          : (H.lang === "ar" ? "التبديل إلى الوضع الفاتح" : "Switch to light theme")
      );
    }
  }

  applyTheme(savedTheme);

  if (themeToggle) {
    themeToggle.addEventListener("click", function () {
      var current = H.getAttribute("data-theme") === "light" ? "light" : "dark";
      var next = current === "light" ? "dark" : "light";
      applyTheme(next);
    });
  }

  var savedLang = null;
  try {
    savedLang = localStorage.getItem("warsha_lang");
  } catch (e) {
    // ignore storage errors
  }

  var urlParams = new URLSearchParams(window.location.search);
  var queryLang = urlParams.get("lang");
  if (queryLang === "en" || queryLang === "ar") {
    savedLang = queryLang;
  }

  if (savedLang === "en") {
    H.lang = "en";
    H.dir = "ltr";
  } else if (savedLang === "ar") {
    H.lang = "ar";
    H.dir = "rtl";
  }

  var langBtn = document.getElementById("lang");
  var root = document.getElementById("root");

  if (langBtn) {
    langBtn.addEventListener("click", function () {
      var next = H.lang === "ar" ? "en" : "ar";
      if (R || !root) {
        setLang(next);
        return;
      }
      root.classList.add("out");
      setTimeout(function () {
        setLang(next);
        root.classList.remove("out");
      }, 200);
    });
  }

  type();

  // Mobile navigation hamburger
  var navToggle = document.getElementById("navToggle");
  var navLinks = document.getElementById("navLinks");
  if (navToggle && navLinks) {
    navToggle.addEventListener("click", function () {
      var isOpen = navLinks.classList.toggle("open");
      navToggle.setAttribute("aria-expanded", isOpen ? "true" : "false");
    });

    var navAnchors = navLinks.querySelectorAll("a");
    navAnchors.forEach(function (a) {
      a.addEventListener("click", function () {
        navLinks.classList.remove("open");
        navToggle.setAttribute("aria-expanded", "false");
      });
    });
  }

  // Reveal on scroll
  var revealElements = document.querySelectorAll(".reveal");
  if (revealElements.length > 0 && "IntersectionObserver" in window) {
    var revealObserver = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add("in");
            revealObserver.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.12 }
    );
    revealElements.forEach(function (el) {
      revealObserver.observe(el);
    });
  } else {
    revealElements.forEach(function (el) {
      el.classList.add("in");
    });
  }

  // Workbench mock sequence
  var mock = document.getElementById("mock");
  var logDivs = document.querySelectorAll("#log div");
  var agentRows = document.querySelectorAll("[data-a]");
  var phoneScr = document.getElementById("scr");
  var played = false;

  function playMock() {
    if (played) return;
    played = true;

    logDivs.forEach(function (row, idx) {
      setTimeout(
        function () {
          row.classList.add("on");
          agentRows.forEach(function (ag) {
            ag.classList.remove("busy");
            var st = ag.querySelector(".st");
            if (st) st.textContent = "done";
          });
          var currentAgent = agentRows[Math.min(idx, agentRows.length - 1)];
          if (idx < 6 && currentAgent) {
            currentAgent.classList.add("busy");
            var st = currentAgent.querySelector(".st");
            if (st) st.textContent = "working";
          }
        },
        R ? 0 : idx * 600
      );
    });

    setTimeout(
      function () {
        if (phoneScr) phoneScr.classList.add("on");
      },
      R ? 0 : 1300
    );
  }

  if (mock && "IntersectionObserver" in window) {
    new IntersectionObserver(
      function (entries) {
        if (entries[0].isIntersecting) {
          playMock();
        }
      },
      { threshold: 0.25 }
    ).observe(mock);
  } else if (mock) {
    playMock();
  }

  // Approval buttons
  var appr = document.getElementById("appr");
  var okBtn = document.getElementById("ok");
  var noBtn = document.getElementById("no");

  if (okBtn && appr) {
    okBtn.addEventListener("click", function () {
      appr.classList.add("done");
    });
  }

  if (noBtn && appr) {
    noBtn.addEventListener("click", function () {
      if (appr.animate && !R) {
        appr.animate(
          [
            { transform: "translateX(0)" },
            { transform: "translateX(5px)" },
            { transform: "translateX(-5px)" },
            { transform: "translateX(0)" },
          ],
          { duration: 250 }
        );
      }
    });
  }

  // Sticky story steps rail
  var steps = document.querySelectorAll(".stp");
  var progLinks = document.querySelectorAll("#prog a");
  var progressBar = document.getElementById("pbar");

  if (steps.length > 0 && "IntersectionObserver" in window) {
    var stepObserver = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            var idx = Array.prototype.indexOf.call(steps, entry.target);
            progLinks.forEach(function (link, j) {
              link.classList.toggle("on", j === idx);
            });
            if (progressBar) {
              progressBar.style.width = ((idx + 1) / steps.length) * 100 + "%";
            }
          }
        });
      },
      { rootMargin: "-40% 0px -40% 0px" }
    );
    steps.forEach(function (s) {
      stepObserver.observe(s);
    });
  }

  // Inside Warsha: AI Factory Visualizer
  var factoryVisualizer = document.getElementById("factoryVisualizer");
  var stepNodes = document.querySelectorAll(".station-node");
  var stationPanels = document.querySelectorAll(".station-panel");
  var conduitFill = document.getElementById("conduitFill");
  var factoryPlayPause = document.getElementById("factoryPlayPause");
  var factorySimulate = document.getElementById("factorySimulate");
  var factoryTrack = document.getElementById("factoryTrack");

  var currentStation = 0;
  var factoryTimer = null;
  var simInterval = null;
  var isPaused = false;
  var isSimulating = false;
  var totalStations = stepNodes.length;

  function cancelSimulation() {
    if (simInterval) {
      clearInterval(simInterval);
      simInterval = null;
    }
    isSimulating = false;
    if (factoryTrack) factoryTrack.classList.remove("simulating");
  }

  function setStation(idx) {
    currentStation = idx;
    stepNodes.forEach(function (node, i) {
      var active = i === idx;
      node.classList.toggle("active", active);
      node.setAttribute("aria-selected", active ? "true" : "false");
    });

    stationPanels.forEach(function (panel, i) {
      var active = i === idx;
      panel.classList.toggle("active", active);
    });

    if (conduitFill && totalStations > 1) {
      var pct = (idx / (totalStations - 1)) * 100;
      conduitFill.style.width = pct + "%";
    }
  }

  function nextStation() {
    var next = (currentStation + 1) % totalStations;
    setStation(next);
  }

  function startFactoryCycle() {
    stopFactoryCycle();
    if (R || isPaused || isSimulating) return;
    factoryTimer = setInterval(nextStation, 3500);
  }

  function stopFactoryCycle() {
    if (factoryTimer) {
      clearInterval(factoryTimer);
      factoryTimer = null;
    }
  }

  if (stepNodes.length > 0) {
    setStation(0);

    stepNodes.forEach(function (node) {
      node.addEventListener("click", function () {
        var step = parseInt(node.getAttribute("data-step"), 10);
        if (!isNaN(step)) {
          cancelSimulation();
          setStation(step);
          if (!isPaused) {
            startFactoryCycle();
          }
        }
      });
    });

    if (factoryTrack) {
      factoryTrack.addEventListener("keydown", function (e) {
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          var stepDelta = H.dir === "rtl" ? (e.key === "ArrowLeft" ? 1 : -1) : (e.key === "ArrowRight" ? 1 : -1);
          var next = (currentStation + stepDelta + totalStations) % totalStations;
          cancelSimulation();
          setStation(next);
          if (stepNodes[next]) stepNodes[next].focus();
          if (!isPaused) startFactoryCycle();
        }
      });
    }

    if (factoryPlayPause) {
      factoryPlayPause.addEventListener("click", function () {
        cancelSimulation();
        isPaused = !isPaused;
        factoryPlayPause.classList.toggle("paused", isPaused);
        if (isPaused) {
          stopFactoryCycle();
        } else {
          startFactoryCycle();
        }
      });
    }

    if (factorySimulate) {
      factorySimulate.addEventListener("click", function () {
        cancelSimulation();
        stopFactoryCycle();
        isSimulating = true;
        if (factoryTrack) factoryTrack.classList.add("simulating");

        if (R) {
          setStation(totalStations - 1);
          cancelSimulation();
          isPaused = true;
          if (factoryPlayPause) factoryPlayPause.classList.add("paused");
          return;
        }

        var step = 0;
        setStation(0);

        simInterval = setInterval(function () {
          step++;
          if (step < totalStations) {
            setStation(step);
          } else {
            cancelSimulation();
            isPaused = true;
            if (factoryPlayPause) factoryPlayPause.classList.add("paused");
          }
        }, 650);
      });
    }

    if (factoryVisualizer && "IntersectionObserver" in window) {
      var factoryObserver = new IntersectionObserver(
        function (entries) {
          if (entries[0].isIntersecting) {
            startFactoryCycle();
          } else {
            stopFactoryCycle();
          }
        },
        { threshold: 0.15 }
      );
      factoryObserver.observe(factoryVisualizer);
    } else {
      startFactoryCycle();
    }
  }
})();
