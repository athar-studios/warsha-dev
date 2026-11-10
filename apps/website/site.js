/* Warsha website — Design System v3 «حِرفة». No dependencies, no network calls. */
(() => {
  'use strict';

  const root = document.documentElement;
  const $ = (sel, ctx = document) => ctx.querySelector(sel);
  const $$ = (sel, ctx = document) => Array.from(ctx.querySelectorAll(sel));
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const money = (v) => `$${v.toFixed(2)}`;
  const lang = () => (root.lang === 'en' ? 'en' : 'ar');
  const bi = (pair) => pair[lang()];
  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

  const store = {
    get(k) {
      try {
        return localStorage.getItem(k);
      } catch {
        return null;
      }
    },
    set(k, v) {
      try {
        localStorage.setItem(k, v);
      } catch {
        /* private mode — preference simply does not persist */
      }
    },
  };

  /* ── icons ──────────────────────────────────────────────────── */
  const icon = (id, cls = '') =>
    `<svg class="ic ${cls}" aria-hidden="true"><use href="#i-${id}"/></svg>`;

  /* ── toasts ─────────────────────────────────────────────────── */
  const toastStack = $('#toasts');
  function toast(text, kind = 'info') {
    const el = document.createElement('div');
    el.className = `toast is-${kind}`;
    const glyph = kind === 'ok' ? 'check' : kind === 'bad' ? 'x' : kind === 'warn' ? 'alert' : 'info';
    el.innerHTML = `${icon(glyph)}<span></span>`;
    el.lastElementChild.textContent = text;
    toastStack.append(el);
    const life = kind === 'bad' ? 7000 : 4200;
    setTimeout(() => {
      el.classList.add('is-out');
      setTimeout(() => el.remove(), 320);
    }, life);
    while (toastStack.children.length > 3) toastStack.firstElementChild.remove();
  }

  /* ── sound: synthesised, tied to visible events, one mute ───── */
  const sound = {
    on: store.get('warsha.sound') !== 'off',
    ctx: null,
    arm() {
      if (this.ctx || !this.on) return;
      const AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) return;
      this.ctx = new AC();
    },
    blip(freq, dur = 0.07, gain = 0.045, type = 'sine') {
      if (!this.on) return;
      this.arm();
      const ctx = this.ctx;
      if (!ctx) return;
      if (ctx.state === 'suspended') ctx.resume();
      const osc = ctx.createOscillator();
      const amp = ctx.createGain();
      osc.type = type;
      osc.frequency.value = freq;
      amp.gain.setValueAtTime(0.0001, ctx.currentTime);
      amp.gain.exponentialRampToValueAtTime(gain, ctx.currentTime + 0.012);
      amp.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + dur);
      osc.connect(amp).connect(ctx.destination);
      osc.start();
      osc.stop(ctx.currentTime + dur + 0.02);
    },
    stage() {
      this.blip(520, 0.06, 0.035, 'triangle');
    },
    gate() {
      this.blip(392, 0.13, 0.05, 'sine');
      setTimeout(() => this.blip(311, 0.16, 0.045, 'sine'), 110);
    },
    approve() {
      this.blip(660, 0.08, 0.04, 'triangle');
      setTimeout(() => this.blip(880, 0.11, 0.035, 'triangle'), 80);
    },
    done() {
      [523, 659, 784, 1047].forEach((f, i) => setTimeout(() => this.blip(f, 0.13, 0.038, 'sine'), i * 95));
    },
    stop() {
      this.blip(180, 0.16, 0.04, 'sawtooth');
    },
  };

  /* ── theme ──────────────────────────────────────────────────── */
  const themeSeg = $('#themeSeg');
  const mq = matchMedia('(prefers-color-scheme: dark)');
  function applyTheme(choice) {
    const dark = choice === 'dark' || (choice === 'system' && mq.matches);
    root.dataset.theme = dark ? 'dark' : 'light';
    root.dataset.themeChoice = choice;
    $$('button', themeSeg).forEach((b) =>
      b.setAttribute('aria-checked', String(b.dataset.themeChoice === choice)),
    );
    const meta = $('meta[name="theme-color"]');
    if (meta) meta.setAttribute('content', dark ? '#131210' : '#F6F4EF');
    store.set('warsha.theme', choice);
  }
  themeSeg.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-theme-choice]');
    if (!btn) return;
    applyTheme(btn.dataset.themeChoice);
    sound.blip(700, 0.05, 0.03, 'triangle');
  });
  themeSeg.addEventListener('keydown', (e) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    const btns = $$('button', themeSeg);
    const i = btns.indexOf(document.activeElement);
    if (i < 0) return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    const next =
      e.key === 'Home' ? 0 : e.key === 'End' ? btns.length - 1 : (i + (root.dir === 'rtl' ? -dir : dir) + btns.length) % btns.length;
    btns[next].focus();
    applyTheme(btns[next].dataset.themeChoice);
  });
  mq.addEventListener('change', () => {
    if ((store.get('warsha.theme') || 'system') === 'system') applyTheme('system');
  });
  applyTheme(store.get('warsha.theme') || root.dataset.themeChoice || 'system');

  /* ── language is owned by the URL; this only localises JS-side strings ── */
  const idea = $('#idea');
  function applyLang(next) {
    idea.placeholder =
      next === 'ar'
        ? 'مثال: تطبيق لعيادة أسنان، المرضى يحجزون مواعيدهم من الموبايل…'
        : 'e.g. an app for a dental clinic where patients book appointments from their phone…';
    $('#gateNote').placeholder =
      next === 'ar' ? 'أضف جدولًا للفواتير، واربطه بالموعد.' : 'Add an invoices table and link it to the appointment.';
    $('#burger').setAttribute('aria-label', next === 'ar' ? 'فتح القائمة' : 'Open menu');
    $('#stepBtn').setAttribute('aria-label', next === 'ar' ? 'المرحلة التالية' : 'Next stage');
    $('#resetBtn').setAttribute('aria-label', next === 'ar' ? 'إعادة التشغيل من البداية' : 'Restart from the beginning');
    $('#track').setAttribute('aria-label', next === 'ar' ? 'مراحل الإنتاج' : 'Production stages');
    $('.controls').setAttribute('aria-label', next === 'ar' ? 'لوحة تحكم المحاكاة' : 'Simulation control desk');
    $('#machineLog').setAttribute('aria-label', next === 'ar' ? 'سجل التشغيل' : 'Run log');
    renderStage();
    renderTeam();
    renderRoad();
    renderFaq();
    renderBench();
    syncSoundBtn();
    updatePlayLabel();
    if (sim.status !== 'idle') setNote(noteForStatus());
  }

  /* ── nav: burger, scroll-spy, progress ──────────────────────── */
  const burger = $('#burger');
  const navLinks = $('#navLinks');
  burger.addEventListener('click', () => {
    const open = burger.getAttribute('aria-expanded') === 'true';
    burger.setAttribute('aria-expanded', String(!open));
    navLinks.classList.toggle('is-open', !open);
  });
  navLinks.addEventListener('click', (e) => {
    if (e.target.closest('a')) {
      burger.setAttribute('aria-expanded', 'false');
      navLinks.classList.remove('is-open');
    }
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && navLinks.classList.contains('is-open')) {
      navLinks.classList.remove('is-open');
      burger.setAttribute('aria-expanded', 'false');
      burger.focus();
    }
  });

  const scrollBar = $('#scrollBar');
  const spyLinks = $$('[data-nav]');
  const spyTargets = spyLinks.map((a) => $(`#${a.dataset.nav}`)).filter(Boolean);
  let ticking = false;
  function onScroll() {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(() => {
      const max = document.body.scrollHeight - innerHeight;
      scrollBar.style.inlineSize = `${max > 0 ? clamp((scrollY / max) * 100, 0, 100) : 0}%`;
      const line = scrollY + parseFloat(getComputedStyle(root).getPropertyValue('--w-nav-height')) + 80;
      let current = null;
      for (const sec of spyTargets) if (sec.offsetTop <= line) current = sec.id;
      spyLinks.forEach((a) => a.setAttribute('aria-current', String(a.dataset.nav === current)));
      ticking = false;
    });
  }
  addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  /* ── reveals + counters ─────────────────────────────────────── */
  const io = new IntersectionObserver(
    (entries) => {
      entries.forEach((en) => {
        if (!en.isIntersecting) return;
        en.target.classList.add('is-in');
        io.unobserve(en.target);
      });
    },
    { rootMargin: '0px 0px -8% 0px', threshold: 0.08 },
  );
  $$('.reveal').forEach((el, i) => {
    el.style.transitionDelay = `${Math.min(i % 4, 3) * 70}ms`;
    io.observe(el);
  });

  const counters = $$('[data-count]');
  const cio = new IntersectionObserver(
    (entries) => {
      entries.forEach((en) => {
        if (!en.isIntersecting) return;
        cio.unobserve(en.target);
        const target = Number(en.target.dataset.count);
        if (reduced()) {
          en.target.textContent = String(target);
          return;
        }
        const t0 = performance.now();
        const dur = 1100;
        const tick = (t) => {
          const p = clamp((t - t0) / dur, 0, 1);
          const eased = 1 - Math.pow(1 - p, 3);
          en.target.textContent = String(Math.round(target * eased));
          if (p < 1) requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      });
    },
    { threshold: 0.5 },
  );
  counters.forEach((c) => {
    // Prerendered pages already carry the real number; do not flash it back to zero.
    if (c.textContent.trim() === c.dataset.count) return;
    c.textContent = '0';
    cio.observe(c);
  });

  /* ── content data ───────────────────────────────────────────── */
  const SCENARIOS = [
    {
      ar: 'عيادة أسنان — حجز مواعيد من الموبايل',
      en: 'Dental clinic — patients book from their phone',
      ideaAr: 'تطبيق لعيادة أسنان، المرضى يحجزون مواعيدهم من الموبايل ويصلهم تذكير قبل الزيارة.',
      ideaEn: 'An app for a dental clinic where patients book from their phone and get a reminder before the visit.',
      domain: 'clinic',
      entities: ['patients', 'appointments', 'reminders'],
    },
    {
      ar: 'سيستم كاشير — فواتير ومخزن لمحل صغير',
      en: 'Point of sale — invoices and stock for a small shop',
      ideaAr: 'سيستم كاشير لمحل صغير، يطبع فاتورة ويخصم القطعة من المخزن ويعرض مبيعات اليوم.',
      ideaEn: 'A point-of-sale system for a small shop that prints an invoice, deducts stock and shows today’s sales.',
      domain: 'pos',
      entities: ['products', 'invoices', 'stock_moves'],
    },
    {
      ar: 'متجر عطور — طلبات وتوصيل وتتبع',
      en: 'Perfume store — orders, delivery and tracking',
      ideaAr: 'متجر عطور على الإنترنت، الزبون يطلب ويدفع ويتابع الشحنة حتى باب البيت.',
      ideaEn: 'An online perfume store where the customer orders, pays and follows the shipment to the door.',
      domain: 'shop',
      entities: ['products', 'orders', 'shipments'],
    },
    {
      ar: 'مدرسة خصوصية — حصص وحضور ومدفوعات',
      en: 'Tutoring school — sessions, attendance, payments',
      ideaAr: 'نظام لمدرسة خصوصية، يحجز الحصص ويسجل الحضور ويتابع مدفوعات أولياء الأمور.',
      ideaEn: 'A system for a tutoring school that books sessions, records attendance and tracks parent payments.',
      domain: 'school',
      entities: ['students', 'sessions', 'payments'],
    },
  ];

  const STAGES = [
    {
      tech: 'intake · research',
      icon: 'chat',
      title: { ar: 'نسمع الفكرة ونسأل عن الناقص', en: 'We hear the idea and ask what is missing' },
      body: {
        ar: 'وكيل الاستقبال يقرأ طلبك، والباحث يجمع ما يلزم. إن نقص شيء يسألونك سؤالًا واحدًا واضحًا قبل أن يبدأ أي عمل.',
        en: 'The intake agent reads your request and the researcher gathers what is needed. If something is missing they ask one clear question before any work starts.',
      },
      out: {
        ar: ['ملخّص الفكرة بلغتك', 'سؤال توضيحي واحد إن لزم', 'بحث قصير عن الحلول المشابهة'],
        en: ['A summary of the idea in your words', 'One clarifying question if needed', 'A short scan of comparable solutions'],
      },
      gate: {
        q: { ar: 'هل هذا هو ما تريده فعلًا؟', en: 'Is this what you actually want?' },
        why: {
          ar: 'الفريق كتب ملخّص الفكرة. موافقتك تعني انتقاله إلى كتابة المتطلبات والتصميم.',
          en: 'The team wrote the idea summary. Approving moves it on to the spec and the design.',
        },
      },
      logs: [
        ['intake', 'read request · {n} chars'],
        ['researcher', 'scan comparable products · 6 hits'],
        ['intake', 'clarifying question queued'],
      ],
    },
    {
      tech: 'prd_spec · design · plan_tasks',
      icon: 'file',
      title: { ar: 'نكتب ما سنبنيه بالضبط', en: 'We write down exactly what gets built' },
      body: {
        ar: 'مدير المنتج يحوّل الفكرة إلى متطلبات مرقّمة، والمصمّم يرسم الشاشات، والمعماري يقسّم العمل إلى مهام صغيرة قابلة للاختبار.',
        en: 'The PM turns the idea into numbered requirements, the designer draws the screens, and the architect splits the work into small testable tasks.',
      },
      out: {
        ar: ['مستند متطلبات مرقّم', 'تصميم الشاشات وقاعدة البيانات', 'قائمة مهام مرتّبة بالتبعية'],
        en: ['A numbered requirements document', 'Screen and database design', 'A task list ordered by dependency'],
      },
      gate: {
        q: { ar: 'هل نعتمد الخطة ونبدأ البناء؟', en: 'Approve the plan and start building?' },
        why: {
          ar: 'المتطلبات والتصميم جاهزان. الموافقة تعني بدء كتابة الكود وصرف فعلي من ميزانيتك.',
          en: 'The spec and design are ready. Approving starts real code writing and real spend from your budget.',
        },
      },
      logs: [
        ['pm', 'spec drafted · 14 requirements'],
        ['designer', 'screens: 6 · tables: {tables}'],
        ['architect', 'task graph · 23 nodes · 0 cycles'],
      ],
    },
    {
      tech: 'code',
      icon: 'code',
      title: { ar: 'نبني بالتوازي، كل مبرمج في مسار', en: 'We build in parallel, one lane per coder' },
      body: {
        ar: 'المبرمجون يعملون في مسارات معزولة حتى لا يكتب أحدهم فوق الآخر. كل مهمة تنتهي باختبار يشغَل قبل أن تُدمج.',
        en: 'Coders work in isolated lanes so none overwrites another. Every task ends with a test that runs before it merges.',
      },
      out: {
        ar: ['كود في مسار معزول لكل مهمة', 'اختبارات تُشغَّل مع كل دفعة', 'طلبات دمج جاهزة للمراجعة'],
        en: ['Code in an isolated lane per task', 'Tests run on every batch', 'Pull requests ready for review'],
      },
      gate: null,
      logs: [
        ['architect', 'schema applied · {tables} tables'],
        ['coder', 'lane {lane} · {entity} module'],
        ['coder', 'tests green · commit staged'],
      ],
    },
    {
      tech: 'review_qa_security',
      icon: 'shield',
      title: { ar: 'نفحص قبل أن نطلب موافقتك', en: 'We check before we ask you' },
      body: {
        ar: 'المراجع يقرأ الكود، ووكيل الجودة يشغّل الاختبارات، ووكيل الأمان يبحث عن تسريب مفاتيح أو صلاحيات زائدة. الفشل يعود للمبرمج تلقائيًا.',
        en: 'The reviewer reads the code, QA runs the tests, and the security agent hunts leaked keys or excess permissions. Failures go back to the coder automatically.',
      },
      out: {
        ar: ['مراجعة كود مكتوبة', 'تقرير اختبارات بالأرقام', 'فحص أمان بلا أسرار مسرّبة'],
        en: ['A written code review', 'A test report with real numbers', 'A security pass with no leaked secrets'],
      },
      gate: {
        q: { ar: 'هل ندمج هذا الكود؟', en: 'Merge this code?' },
        why: {
          ar: 'كل الفحوص انتهت. الموافقة تدمج التغييرات في الفرع الرئيسي وتجهّز الإصدار.',
          en: 'Every check has finished. Approving merges the changes into the main branch and prepares the release.',
        },
      },
      logs: [
        ['reviewer', 'diff read · 3 comments'],
        ['qa', 'suite run · 42 passed · 0 failed'],
        ['security', 'secret scan · clean'],
      ],
    },
    {
      tech: 'release · launch_drafts · feedback',
      icon: 'box',
      title: { ar: 'نجهّز التسليم وننتظر كلمتك', en: 'We prepare delivery and wait for your word' },
      body: {
        ar: 'وكيل الإصدار يبني الحزمة ويكتب ملاحظات النسخة، ووكيل التسويق يجهّز نص المتجر. لا شيء يُنشر قبل موافقتك على بوابة النشر.',
        en: 'The release agent builds the package and writes the changelog; the marketer drafts the store listing. Nothing publishes before you approve the release gate.',
      },
      out: {
        ar: ['حزمة قابلة للتثبيت', 'ملاحظات نسخة ونصوص المتجر', 'قناة ملاحظات المستخدمين'],
        en: ['An installable package', 'Changelog and store copy', 'A user-feedback channel'],
      },
      gate: {
        q: { ar: 'هل ننشر النسخة الآن؟', en: 'Publish this version now?' },
        why: {
          ar: 'هذه آخر بوابة. الموافقة تعني أن الإصدار يصبح متاحًا للناس خارج جهازك.',
          en: 'This is the last gate. Approving means the release becomes available to people outside your machine.',
        },
      },
      logs: [
        ['release', 'package built · checksum written'],
        ['marketer', 'store copy drafted · ar + en'],
        ['feedback', 'channel opened · awaiting launch'],
      ],
    },
  ];

  const ROLES = [
    { ar: 'الاستقبال', en: 'Intake', icon: 'chat', hue: 'info', stage: 'intake', desc: { ar: 'يقرأ طلبك ويسأل عن الناقص قبل أن يبدأ الفريق.', en: 'Reads your request and asks what is missing before the team starts.' } },
    { ar: 'الباحث', en: 'Researcher', icon: 'search', hue: 'info', stage: 'research', desc: { ar: 'يجمع الحلول المشابهة وما يلزم من معلومات.', en: 'Gathers comparable solutions and the information needed.' } },
    { ar: 'مدير المنتج', en: 'Product manager', icon: 'file', hue: 'primary', stage: 'prd_spec', desc: { ar: 'يحوّل الفكرة إلى متطلبات مرقّمة قابلة للقياس.', en: 'Turns the idea into numbered, measurable requirements.' } },
    { ar: 'المصمّم', en: 'Designer', icon: 'pen', hue: 'primary', stage: 'design', desc: { ar: 'يرسم الشاشات وقاعدة البيانات قبل كتابة الكود.', en: 'Draws the screens and the database before code is written.' } },
    { ar: 'المعماري', en: 'Architect', icon: 'layers', hue: 'attention', stage: 'plan_tasks', desc: { ar: 'يقسّم العمل مهامًا صغيرة ويرتّبها بالتبعية.', en: 'Splits the work into small tasks ordered by dependency.' } },
    { ar: 'المبرمج', en: 'Coder', icon: 'code', hue: 'attention', stage: 'code', desc: { ar: 'يكتب الكود في مسار معزول مع اختبار لكل مهمة.', en: 'Writes code in an isolated lane with a test per task.' } },
    { ar: 'المراجع', en: 'Reviewer', icon: 'eye', hue: 'success', stage: 'review', desc: { ar: 'يقرأ كل تغيير ويكتب ملاحظاته قبل الدمج.', en: 'Reads every change and writes notes before the merge.' } },
    { ar: 'الجودة', en: 'QA', icon: 'check', hue: 'success', stage: 'qa', desc: { ar: 'يشغّل الاختبارات ويعيد الفشل إلى المبرمج.', en: 'Runs the tests and sends failures back to the coder.' } },
    { ar: 'الأمان', en: 'Security', icon: 'shield', hue: 'danger', stage: 'security', desc: { ar: 'يبحث عن أسرار مسرّبة وصلاحيات أوسع من اللازم.', en: 'Hunts leaked secrets and permissions wider than needed.' } },
    { ar: 'الإصدار', en: 'Release', icon: 'box', hue: 'danger', stage: 'release', desc: { ar: 'يبني الحزمة ويكتب ملاحظات النسخة.', en: 'Builds the package and writes the changelog.' } },
    { ar: 'التسويق', en: 'Marketer', icon: 'megaphone', hue: 'info', stage: 'launch_drafts', desc: { ar: 'يجهّز نص المتجر بالعربية والإنجليزية.', en: 'Drafts the store listing in Arabic and English.' } },
    { ar: 'الملاحظات', en: 'Feedback', icon: 'chat', hue: 'success', stage: 'feedback', desc: { ar: 'يجمع كلام المستخدمين ويحوّله إلى مهام.', en: 'Collects what users say and turns it into tasks.' } },
  ];

  const HUES = {
    primary: ['var(--w-primary-soft)', 'var(--w-primary-text)'],
    attention: ['var(--w-attention-soft)', 'var(--w-attention)'],
    success: ['var(--w-success-soft)', 'var(--w-success)'],
    danger: ['var(--w-danger-soft)', 'var(--w-danger)'],
    info: ['var(--w-info-soft)', 'var(--w-info)'],
  };

  const ROAD = [
    { id: 'M0', s: 'done', ar: ['أساس المشروع والفحوص الآلية', 'هيكل المستودع وبوابات التحقق تعمل منذ اليوم الأول.'], en: ['Foundation and automated checks', 'Repository structure and verification gates have run since day one.'] },
    { id: 'M1', s: 'done', ar: ['المحرّك الخلفي والحالة المحفوظة', 'خدمة تعمل في الخلفية وتحفظ كل شيء على جهازك.'], en: ['Background engine and saved state', 'A background service that keeps everything on your machine.'] },
    { id: 'M2', s: 'done', ar: ['المزوّدون والميزانية والتكلفة', 'تربط مزوّدك، وتضع حدّ إنفاق، ويتوقف الفريق قبل تجاوزه.'], en: ['Providers, budgets and cost', 'Connect your provider, set a cap, and the team stops before crossing it.'] },
    { id: 'M3', s: 'done', ar: ['خط الإنتاج وبوابات الموافقة', 'أربع عشرة مرحلة وأربع بوابات لا يتجاوزها الفريق بدونك.'], en: ['The pipeline and approval gates', 'Fourteen stages and four gates the team never crosses without you.'] },
    { id: 'M4', s: 'done', ar: ['أداة سطر الأوامر', 'أوامر للموافقة والاقتران والمتابعة من الطرفية.'], en: ['The command-line tool', 'Commands to approve, pair and follow along from a terminal.'] },
    { id: 'M5', s: 'done', ar: ['تطبيق سطح المكتب', 'يعمل على ويندوز ويُبنى من المصدر على البقية.'], en: ['The desktop app', 'Runs on Windows and builds from source elsewhere.'] },
    { id: 'M6', s: 'part', ar: ['تطبيق الهاتف والاقتران الآمن', 'الاقتران يعمل. القناة المشفّرة بين الجهازين غير منفّذة بعد.'], en: ['The phone app and secure pairing', 'Pairing works. The encrypted relay between devices is not implemented yet.'] },
    { id: 'M7', s: 'part', ar: ['وكلاء البرمجة وتكامل GitHub', 'الخلفيات جاهزة وعميل GitHub موجود، لكن لا شاشة تقوده بعد.'], en: ['Coding agents and GitHub', 'Backends are ready and the GitHub client exists, but no screen drives it yet.'] },
    { id: 'M8', s: 'done', ar: ['عربي وإنجليزي وتخطيط من اليمين', 'الكتالوجان متطابقان، والتخطيط ينعكس بالكامل.'], en: ['Arabic, English and RTL', 'Both catalogs match and the layout mirrors fully.'] },
    { id: 'M9', s: 'part', ar: ['المثبّتات والإصدارات والتوثيق', 'مثبّت محلي يُبنى. لا يوجد إصدار منشور حتى الآن.'], en: ['Installers, releases and docs', 'A local installer builds. No release has been published yet.'] },
  ];

  const FAQ = [
    {
      ar: ['كم يكلّفني؟', 'ورشة نفسها لا تأخذ منك رسومًا. تستخدم مزوّد الذكاء الاصطناعي الخاص بك، فالتكلفة هي ما تدفعه له أصلًا. يظهر عدّاد تكلفة أمامك، وتضع حدّ إنفاق فيتوقف الفريق قبل تجاوزه.'],
      en: ['What does it cost me?', 'Warsha itself charges nothing. You use your own AI provider, so the cost is what you already pay them. A cost meter stays in front of you, and you set a cap the team stops before crossing.'],
    },
    {
      ar: ['أين تعيش بياناتي؟', 'على جهازك. المشروع والكود والسجلّات تُحفظ محليًا، ومفاتيح المزوّدين تُحفظ في سلسلة مفاتيح نظام التشغيل ولا تُطبع في أي سجل. ما يغادر جهازك هو طلباتك إلى مزوّد الذكاء الاصطناعي الذي اخترته.'],
      en: ['Where does my data live?', 'On your machine. Projects, code and logs are stored locally, and provider keys live in the OS keychain and are never printed to a log. What leaves your machine are the requests to the AI provider you chose.'],
    },
    {
      ar: ['هل يستطيع الفريق النشر بدوني؟', 'لا. أربع بوابات تقف عندها كل مرة: الفكرة، المواصفات، دمج الكود، النشر. آخر بوابة هي النشر، ولا شيء يخرج إلى الناس قبل أن تضغط «موافق».'],
      en: ['Can the team publish without me?', 'No. Four gates stop it every time: the idea, the spec, the merge, the release. The last gate is publishing, and nothing reaches people before you press approve.'],
    },
    {
      ar: ['ماذا لو لم يعجبني ما فعله؟', 'اضغط «اطلب تعديلًا» واكتب ما تريده في الحقل. يعود الفريق إلى المرحلة نفسها ويعيد العمل على أساس ملاحظتك. الرفض يوقف التشغيل ويحفظ كل ما أُنجز.'],
      en: ['What if I do not like the result?', 'Press “request changes” and write what you want. The team returns to the same stage and reworks it around your note. Rejecting stops the run and keeps everything already produced.'],
    },
    {
      ar: ['هل هذا مفتوح المصدر؟', 'الكود متاح للجميع للقراءة والبناء والتعديل والمشاركة، لكنه ليس مفتوح المصدر بالمعنى الرسمي: الرخصة PolyForm Noncommercial 1.0.0 تمنع الاستخدام التجاري. للاستخدام التجاري تواصل مع Athar Studio.'],
      en: ['Is this open source?', 'The code is available for anyone to read, build, modify and share, but it is not open source in the official sense: the PolyForm Noncommercial 1.0.0 license forbids commercial use. For commercial use, contact Athar Studio.'],
    },
    {
      ar: ['هل يوجد إصدار جاهز أحمّله؟', 'لا حتى الآن. تبني التطبيق من المصدر باتباع دليل التثبيت في المستودع. المثبّت المحلي يُبنى، لكن لا نسخة موسومة ولا إصدار منشور.'],
      en: ['Is there a ready release to download?', 'Not yet. You build the app from source following the install guide in the repository. A local installer builds, but there is no tagged version and no published release.'],
    },
  ];

  /* ── renderers ──────────────────────────────────────────────── */
  function renderTeam() {
    $('#teamGrid').innerHTML = ROLES.map((r, i) => {
      const [bg, fg] = HUES[r.hue];
      return `<article class="team-card reveal is-in" style="transition-delay:${(i % 4) * 50}ms">
        <span class="team-mark" style="background:${bg};color:${fg}">${icon(r.icon)}</span>
        <h3>${bi(r)}</h3>
        <p>${bi(r.desc)}</p>
        <span class="team-stage ltr">${r.stage}</span>
      </article>`;
    }).join('');
  }

  function renderRoad() {
    $('#roadList').innerHTML = ROAD.map((m) => {
      const [title, body] = bi(m);
      const cls = m.s === 'done' ? 'is-done' : 'is-part';
      const pill =
        m.s === 'done'
          ? `<span class="pill pill-done pill-xs">${icon('check')}${lang() === 'ar' ? 'مُسلَّم' : 'Delivered'}</span>`
          : `<span class="pill pill-wait pill-xs">${icon('alert')}${lang() === 'ar' ? 'جزئي' : 'Partial'}</span>`;
      return `<li class="${cls}"><span class="road-id ltr">${m.id}</span><div class="road-body"><h3>${title}</h3><p>${body}</p>${pill}</div></li>`;
    }).join('');
  }

  function renderFaq() {
    const open = $$('#faqList details').map((d) => d.open);
    $('#faqList').innerHTML = FAQ.map((f, i) => {
      const [q, a] = bi(f);
      return `<details${open[i] ? ' open' : ''}><summary>${q}${icon('chevron')}</summary><div class="faq-body">${a}</div></details>`;
    }).join('');
  }

  const DETAIL_STAGES = [
    ['intake', 'الاستقبال', 'Intake', 'd'],
    ['research', 'البحث', 'Research', 'd'],
    ['prd_spec', 'المتطلبات', 'Spec', 'd'],
    ['gate_a', 'بوابة أ', 'Gate A', 'd'],
    ['design', 'التصميم', 'Design', 'd'],
    ['plan_tasks', 'المعمارية', 'Architecture', 'c'],
    ['gate_b', 'بوابة ب', 'Gate B', 'w'],
    ['code', 'البرمجة', 'Code', ''],
    ['review', 'المراجعة', 'Review', ''],
    ['qa', 'الجودة', 'QA', ''],
    ['security', 'الأمان', 'Security', ''],
    ['gate_c', 'بوابة ج', 'Gate C', ''],
    ['release', 'الإصدار', 'Release', ''],
    ['gate_d', 'بوابة د', 'Gate D', ''],
  ];

  const TEAM_ROWS = [
    ['architect', 4, 'done', 'deepseek-chat', 0.42],
    ['coder', 5, 'run', 'deepseek-chat', 1.36],
    ['coder', 5, 'run', 'deepseek-chat', 0.88],
    ['coder', 5, 'idle', 'qwen2.5-coder', 0.0],
    ['reviewer', 6, 'idle', 'deepseek-reasoner', 0.31],
    ['qa', 7, 'idle', 'deepseek-chat', 0.23],
  ];

  const COST_SERIES = [
    { ar: 'البرمجة', en: 'Coding', v: 1.36, c: 'var(--w-info)', p: '' },
    { ar: 'المراجعة', en: 'Review', v: 0.62, c: 'var(--w-success)', p: '' },
    { ar: 'التصميم', en: 'Design', v: 0.48, c: 'var(--w-primary-text)', p: '' },
    { ar: 'المعمارية', en: 'Architecture', v: 0.42, c: 'var(--w-attention)', p: '' },
    { ar: 'الجودة', en: 'QA', v: 0.23, c: 'var(--w-danger)', p: '' },
    { ar: 'البحث', en: 'Research', v: 0.09, c: 'var(--w-text-faint)', p: '' },
  ];

  function renderBench() {
    $('#stageChips').innerHTML = DETAIL_STAGES.map(
      ([id, ar, en, s]) => `<span class="${s}" title="${id}">${lang() === 'ar' ? ar : en}</span>`,
    ).join('');

    $('#teamRows').innerHTML = TEAM_ROWS.map(([role, ri, st, model, cost]) => {
      const r = ROLES[ri];
      const [, fg] = HUES[r.hue];
      const pill =
        st === 'done'
          ? `<span class="pill pill-done pill-xs">${icon('check')}${lang() === 'ar' ? 'انتهى' : 'Done'}</span>`
          : st === 'run'
            ? `<span class="pill pill-run pill-xs">${icon('clock')}${lang() === 'ar' ? 'يعمل' : 'Running'}</span>`
            : `<span class="pill pill-xs">${lang() === 'ar' ? 'خامل' : 'Idle'}</span>`;
      return `<tr><td><span class="role-cell"><i class="role-dot" style="background:${fg}"></i>${bi(r)}</span></td><td>${pill}</td><td><code class="ltr">${model}</code></td><td><bdi class="ltr">${money(cost)}</bdi></td></tr>`;
    }).join('');

    const sc = SCENARIOS[Number($('#ctlScenario').value) || 0];
    const files = [
      ['crates/', 0, ''],
      [`${sc.domain}-core/src/lib.rs`, 1, 'mod'],
      [`${sc.domain}-core/src/models.rs`, 1, 'new'],
      [`${sc.domain}-api/src/routes.rs`, 1, 'new'],
      [`${sc.domain}-api/tests/bookings.rs`, 1, 'new'],
      ['apps/mobile/src/screens/', 0, ''],
      ['BookingScreen.tsx', 1, 'new'],
      ['RemindersScreen.tsx', 1, 'mod'],
    ];
    $('#fileTree').innerHTML = files
      .map(
        ([name, depth, tag]) =>
          `<li style="padding-inline-start:${depth * 18}px">${icon(tag ? 'file' : 'layers')}<span>${name}</span>${
            tag ? `<span class="tag-${tag}">${tag === 'new' ? '+ new' : '~ mod'}</span>` : ''
          }</li>`,
      )
      .join('');

    const ent = sc.entities[1];
    $('#diffView').innerHTML = [
      ['hdr', '', `@@ ${sc.domain}-core/src/models.rs @@`],
      ['ctx', ' ', 'pub struct Booking {'],
      ['ctx', ' ', '    pub id: Uuid,'],
      ['del', '-', '    pub date: String,'],
      ['add', '+', '    pub starts_at: DateTime<Utc>,'],
      ['add', '+', '    pub reminder_sent: bool,'],
      ['ctx', ' ', '}'],
      ['hdr', '', `@@ ${sc.domain}-api/tests/${ent}.rs @@`],
      ['add', '+', '#[test] fn rejects_past_bookings() { … }'],
      ['add', '+', '#[test] fn sends_reminder_once() { … }'],
    ]
      .map(([c, g, t]) => `<div class="${c}"><span class="gutter">${g}</span><span>${t.replace(/</g, '&lt;')}</span></div>`)
      .join('');

    $('#termView').innerHTML = [
      `<span class="key">$ cargo test -p ${sc.domain}-core</span>`,
      '   Compiling warsha-core v0.1.0',
      `   Compiling ${sc.domain}-core v0.1.0`,
      '    Finished test [unoptimized + debuginfo] in 4.21s',
      '     Running unittests src/lib.rs',
      '',
      'running 6 tests',
      'test models::rejects_past_bookings ... <span class="ok">ok</span>',
      'test models::sends_reminder_once ... <span class="ok">ok</span>',
      'test routes::books_a_slot ... <span class="ok">ok</span>',
      'test routes::rejects_double_booking ... <span class="ok">ok</span>',
      'test store::persists_locally ... <span class="ok">ok</span>',
      'test budget::halts_at_cap ... <span class="ok">ok</span>',
      '',
      '<span class="ok">test result: ok. 6 passed; 0 failed; 0 ignored</span>',
      '',
      '<span class="warn">gate_b: waiting for owner approval</span>',
    ].join('\n');

    const max = Math.max(...COST_SERIES.map((s) => s.v));
    $('#costChart').innerHTML = COST_SERIES.map(
      (s) => `<div class="cost-row"><span class="cost-name">${bi(s)}</span><span class="cost-track"><i style="background:${s.c}" data-w="${(s.v / max) * 100}"></i></span><span class="cost-val">${money(s.v)}</span></div>`,
    ).join('');
    requestAnimationFrame(() => {
      $$('#costChart .cost-track i').forEach((el) => {
        el.style.inlineSize = `${el.dataset.w}%`;
      });
    });
  }

  /* ── hero composer ──────────────────────────────────────────── */
  const ideaCount = $('#ideaCount');
  const ideaError = $('#ideaError');
  idea.addEventListener('input', () => {
    ideaCount.textContent = `${idea.value.length} / 240`;
    if (idea.value.trim()) {
      ideaError.hidden = true;
      idea.removeAttribute('aria-invalid');
    }
  });

  $$('.chip').forEach((chip) => {
    chip.addEventListener('click', () => {
      const sc = SCENARIOS[Number(chip.dataset.idea)];
      idea.value = lang() === 'ar' ? sc.ideaAr : sc.ideaEn;
      idea.dispatchEvent(new Event('input'));
      $('#ctlScenario').value = chip.dataset.idea;
      sim.scenario = Number(chip.dataset.idea);
      resetSim(false);
      idea.focus();
      sound.blip(560, 0.05, 0.03, 'triangle');
    });
  });

  $('#composer').addEventListener('submit', (e) => {
    e.preventDefault();
    const text = idea.value.trim();
    if (!text) {
      ideaError.hidden = false;
      idea.setAttribute('aria-invalid', 'true');
      idea.focus();
      sound.stop();
      return;
    }
    sim.custom = text;
    resetSim(false);
    startSim();
    $('#factory').scrollIntoView({ behavior: reduced() ? 'auto' : 'smooth', block: 'start' });
    toast(
      lang() === 'ar' ? 'بدأ التشغيل. تابع خط الإنتاج بالأسفل.' : 'Run started. Follow the line below.',
      'ok',
    );
  });

  /* ── the factory simulation ─────────────────────────────────── */
  const sim = {
    scenario: 0,
    custom: '',
    stage: 0,
    progress: 0,
    status: 'idle',
    spent: 0,
    cap: 10,
    speed: 1,
    parallel: 3,
    autoApprove: false,
    runNo: 1,
    log: [],
    lanes: [],
    elapsed: 0,
    marks: new Set(),
  };

  const trackWrap = $('#trackWrap');
  const nodes = $$('.node');
  const machineLog = $('#machineLog');
  const decision = $('#decision');
  const playBtn = $('#playBtn');
  const playUse = $('#playUse');
  const playLabel = $('#playLabel');
  const runPill = $('#runPill');
  const runPillTxt = $('#runPillTxt');

  const PILL = {
    idle: ['', 'clock', { ar: 'جاهزة', en: 'Idle' }],
    running: ['pill-run', 'clock', { ar: 'تعمل', en: 'Running' }],
    waiting: ['pill-wait', 'alert', { ar: 'ينتظرك', en: 'Waiting on you' }],
    paused: ['', 'pause', { ar: 'متوقفة مؤقتًا', en: 'Paused' }],
    done: ['pill-done', 'check', { ar: 'سُلِّمت', en: 'Shipped' }],
    capped: ['pill-fail', 'wallet', { ar: 'بلغت الحد', en: 'Cap reached' }],
  };

  function setPill(status) {
    const [cls, ic, label] = PILL[status];
    runPill.className = `pill ${cls}`;
    runPill.innerHTML = `${icon(ic)}<span id="runPillTxt">${bi(label)}</span>`;
  }

  function noteForStatus() {
    const s = sim.status;
    if (s === 'waiting')
      return bi({ ar: 'الفريق متوقف عند بوابة. القرار لك وحدك.', en: 'The team is stopped at a gate. The decision is yours alone.' });
    if (s === 'done')
      return bi({ ar: 'اكتملت الدورة. كل ما نُشر مرّ من بوابة موافقتك.', en: 'The cycle completed. Everything shipped passed through your approval gate.' });
    if (s === 'capped')
      return bi({ ar: 'بلغ الإنفاق حدّك فتوقّف الفريق. ارفع الحدّ لتكمل.', en: 'Spend reached your cap so the team stopped. Raise the cap to continue.' });
    if (s === 'paused') return bi({ ar: 'متوقفة مؤقتًا. اضغط تشغيل لتكمل من مكانها.', en: 'Paused. Press play to resume where it stopped.' });
    if (s === 'running') return bi({ ar: 'الفريق يعمل الآن. اللون البرتقالي وحده يعني أنه يحتاجك.', en: 'The team is working. Only orange means it needs you.' });
    return bi({ ar: 'اضغط «تشغيل» أو حرّك أي مقبض لتبدأ المحاكاة.', en: 'Press play, or move any control to start the simulation.' });
  }
  function setNote(text) {
    $('#lineNote').textContent = text;
  }

  function updatePlayLabel() {
    const running = sim.status === 'running';
    playUse.setAttribute('href', running ? '#i-pause' : '#i-play');
    playBtn.setAttribute('aria-pressed', String(running));
    playLabel.innerHTML = running
      ? `<span data-ar>إيقاف مؤقت</span><span data-en>Pause</span>`
      : `<span data-ar>${sim.status === 'done' ? 'أعد التشغيل' : 'تشغيل'}</span><span data-en>${sim.status === 'done' ? 'Replay' : 'Play'}</span>`;
  }

  function pushLog(role, msg, kind = '') {
    const stamp = `${String(Math.floor(sim.elapsed / 60)).padStart(2, '0')}:${String(Math.floor(sim.elapsed % 60)).padStart(2, '0')}`;
    sim.log.push({ t: stamp, role, msg, kind, stage: sim.stage });
    const el = document.createElement('div');
    el.className = `log-line ${kind ? `is-${kind}` : ''}`;
    el.innerHTML = `<span class="log-t">${stamp}</span><span class="log-role">${role}</span><span class="log-msg"></span>`;
    el.lastElementChild.textContent = msg;
    machineLog.append(el);
    while (machineLog.children.length > 60) machineLog.firstElementChild.remove();
    machineLog.scrollTop = machineLog.scrollHeight;
  }

  function fillTemplate(s) {
    const sc = SCENARIOS[sim.scenario];
    return s
      .replace('{n}', String((sim.custom || sc.ideaEn).length))
      .replace('{tables}', String(sc.entities.length))
      .replace('{entity}', sc.entities[sim.stage % sc.entities.length])
      .replace('{lane}', String((sim.marks.size % sim.parallel) + 1));
  }

  function artifactNames() {
    const sc = SCENARIOS[sim.scenario];
    return [
      ['idea.md', 'research.md'],
      ['requirements.md', 'schema.sql', 'tasks.json'],
      [`${sc.domain}-core.rs`, `${sc.domain}-api.rs`, 'screens.tsx'],
      ['review.md', 'tests.xml', 'security.md'],
      ['package.zip', 'CHANGELOG.md', 'store-listing.md'],
    ][sim.stage];
  }

  function renderArtifacts() {
    const names = artifactNames();
    const shown = sim.status === 'done' && sim.stage === STAGES.length - 1 ? names.length : sim.marks.size;
    const visible = names.slice(0, clamp(shown, 0, names.length));
    const box = $('#stageArtifacts');
    if (box.dataset.sig === `${sim.stage}:${visible.length}`) return;
    box.dataset.sig = `${sim.stage}:${visible.length}`;
    box.innerHTML = visible
      .map((n, i) => `<span class="artifact" style="animation-delay:${i * 60}ms">${icon('file')}${n}</span>`)
      .join('');
  }

  function renderStage() {
    const st = STAGES[sim.stage];
    if (!st) return;
    $('#stageIndex').textContent = `${String(sim.stage + 1).padStart(2, '0')} / 05`;
    $('#stageTech').textContent = st.tech;
    $('#stageTitle').textContent = bi(st.title);
    $('#stageBody').textContent = bi(st.body);
    $('#stageOut').innerHTML = bi(st.out)
      .map((o, i) => `<li style="animation-delay:${i * 70}ms">${icon('check')}<span>${o}</span></li>`)
      .join('');
    $('#stagePanel').setAttribute('aria-labelledby', `node${sim.stage}`);
    $('#runNo').textContent = String(sim.runNo);
    $('#lineReq').textContent = sim.custom || bi(SCENARIOS[sim.scenario]);

    nodes.forEach((n, i) => {
      const done = i < sim.stage || sim.status === 'done';
      n.classList.toggle('is-done', done);
      n.classList.toggle('is-active', i === sim.stage && sim.status !== 'waiting' && sim.status !== 'done');
      n.classList.toggle('is-waiting', i === sim.stage && sim.status === 'waiting');
      n.setAttribute('aria-selected', String(i === sim.stage));
      n.tabIndex = i === sim.stage ? 0 : -1;
      $('.node-ring use', n).setAttribute('href', done ? '#i-check' : `#i-${STAGES[i].icon}`);
    });

    const overall = ((sim.stage + sim.progress) / STAGES.length) * 100;
    $('#conduitFill').style.inlineSize = `${clamp(overall, 0, 100)}%`;
    $('#miniBar').style.inlineSize = `${clamp(overall, 0, 100)}%`;
    $('#spent').textContent = money(sim.spent);
    $('#cap').textContent = `$${sim.cap}`;
    renderArtifacts();
  }

  function renderLanes() {
    const box = $('#coderLanes');
    if (sim.stage !== 2) {
      box.hidden = true;
      return;
    }
    box.hidden = false;
    if (sim.lanes.length !== sim.parallel) {
      sim.lanes = Array.from({ length: sim.parallel }, () => 0);
      box.innerHTML = sim.lanes
        .map(
          (_, i) =>
            `<div class="lane"><span class="lane-name">coder-${i + 1}</span><span class="lane-bar"><i></i></span></div>`,
        )
        .join('');
    }
    $$('.lane', box).forEach((el, i) => {
      el.classList.toggle('is-done', sim.lanes[i] >= 1);
      $('.lane-bar i', el).style.inlineSize = `${clamp(sim.lanes[i], 0, 1) * 100}%`;
    });
  }

  function showDecision() {
    const gate = STAGES[sim.stage].gate;
    if (!gate) return;
    $('#decisionQ').textContent = bi(gate.q);
    $('#decisionWhy').textContent = bi(gate.why);
    decision.hidden = false;
  }

  function resetSim(bumpRun = true) {
    if (bumpRun) sim.runNo += 1;
    sim.stage = 0;
    sim.progress = 0;
    sim.spent = 0;
    sim.elapsed = 0;
    sim.status = 'idle';
    sim.marks.clear();
    sim.lanes = [];
    sim.log = [];
    machineLog.innerHTML = '';
    decision.hidden = true;
    trackWrap.classList.remove('is-running');
    setPill('idle');
    setNote(noteForStatus());
    updatePlayLabel();
    renderStage();
    renderLanes();
    pushLog('warshad', `run #${sim.runNo} ready · scenario=${SCENARIOS[sim.scenario].domain}`, 'key');
  }

  function startSim() {
    if (sim.status === 'done') resetSim(true);
    if (sim.status === 'waiting') return;
    sim.status = 'running';
    trackWrap.classList.add('is-running');
    setPill('running');
    setNote(noteForStatus());
    updatePlayLabel();
    renderStage();
    sound.arm();
    sound.stage();
    if (!sim.marks.has(0)) {
      sim.marks.add(0);
      const first = STAGES[sim.stage].logs[0];
      pushLog(first[0], fillTemplate(first[1]));
    }
  }

  function pauseSim() {
    if (sim.status !== 'running') return;
    sim.status = 'paused';
    trackWrap.classList.remove('is-running');
    setPill('paused');
    setNote(noteForStatus());
    updatePlayLabel();
  }

  function reachGate() {
    const gate = STAGES[sim.stage].gate;
    if (!gate) return advance();
    if (sim.autoApprove) {
      pushLog('owner', `gate auto-approved · stage=${STAGES[sim.stage].tech}`, 'ok');
      return advance();
    }
    sim.status = 'waiting';
    trackWrap.classList.remove('is-running');
    setPill('waiting');
    setNote(noteForStatus());
    updatePlayLabel();
    renderStage();
    showDecision();
    pushLog('gate', `waiting for owner · ${STAGES[sim.stage].tech}`, 'wait');
    sound.gate();
  }

  function advance() {
    decision.hidden = true;
    if (sim.stage >= STAGES.length - 1) {
      sim.stage = STAGES.length - 1;
      sim.progress = 1;
      sim.status = 'done';
      trackWrap.classList.remove('is-running');
      setPill('done');
      setNote(noteForStatus());
      updatePlayLabel();
      renderStage();
      pushLog('warshad', `run #${sim.runNo} shipped · spent ${money(sim.spent)}`, 'ok');
      sound.done();
      toast(lang() === 'ar' ? 'اكتملت الدورة وسُلِّم المشروع.' : 'The cycle completed and the project shipped.', 'ok');
      return;
    }
    sim.stage += 1;
    sim.progress = 0;
    sim.marks.clear();
    sim.lanes = [];
    sim.status = 'running';
    trackWrap.classList.add('is-running');
    setPill('running');
    setNote(noteForStatus());
    updatePlayLabel();
    renderStage();
    renderLanes();
    sound.stage();
    const first = STAGES[sim.stage].logs[0];
    pushLog(first[0], fillTemplate(first[1]));
  }

  let last = performance.now();
  function loop(now) {
    const dt = Math.min((now - last) / 1000, 0.1);
    last = now;
    if (sim.status === 'running') {
      sim.elapsed += dt * sim.speed;
      const boost = sim.stage === 2 ? 0.7 + sim.parallel * 0.22 : 1;
      sim.progress += dt * sim.speed * 0.26 * boost;

      const rate = sim.stage === 2 ? 0.22 * sim.parallel : 0.16;
      sim.spent = Math.min(sim.cap, sim.spent + dt * sim.speed * rate);
      if (sim.spent >= sim.cap) {
        sim.status = 'capped';
        trackWrap.classList.remove('is-running');
        setPill('capped');
        setNote(noteForStatus());
        updatePlayLabel();
        pushLog('budget', `cap reached at ${money(sim.spent)} · run halted`, 'wait');
        sound.stop();
        toast(lang() === 'ar' ? 'بلغ الإنفاق حدّك. ارفع الحدّ لتكمل.' : 'Spend reached your cap. Raise it to continue.', 'warn');
      }

      const logs = STAGES[sim.stage].logs;
      logs.forEach((entry, i) => {
        const at = (i + 1) / (logs.length + 1);
        if (sim.progress >= at && !sim.marks.has(i)) {
          sim.marks.add(i);
          pushLog(entry[0], fillTemplate(entry[1]), i === logs.length - 1 ? 'ok' : '');
          renderArtifacts();
        }
      });

      if (sim.stage === 2) {
        if (sim.lanes.length !== sim.parallel) renderLanes();
        sim.lanes = sim.lanes.map((v, i) => clamp(v + dt * sim.speed * (0.34 + i * 0.05), 0, 1));
        renderLanes();
      }

      $('#spent').textContent = money(sim.spent);
      const overall = ((sim.stage + clamp(sim.progress, 0, 1)) / STAGES.length) * 100;
      $('#conduitFill').style.inlineSize = `${clamp(overall, 0, 100)}%`;
      $('#miniBar').style.inlineSize = `${clamp(overall, 0, 100)}%`;

      if (sim.progress >= 1 && sim.status === 'running') {
        sim.progress = 1;
        reachGate();
      }
    }
    requestAnimationFrame(loop);
  }

  /* controls */
  playBtn.addEventListener('click', () => {
    if (sim.status === 'running') pauseSim();
    else if (sim.status === 'waiting') {
      toast(lang() === 'ar' ? 'الفريق ينتظر قرارك عند البوابة.' : 'The team is waiting for your decision at the gate.', 'warn');
      $('#approveBtn').focus();
    } else if (sim.status === 'capped') {
      toast(lang() === 'ar' ? 'ارفع حدّ الإنفاق أولًا.' : 'Raise the budget cap first.', 'warn');
      $('#ctlBudget').focus();
    } else startSim();
  });

  $('#stepBtn').addEventListener('click', () => {
    if (sim.status === 'done') resetSim(true);
    decision.hidden = true;
    if (sim.stage >= STAGES.length - 1) {
      sim.progress = 1;
      sim.status = 'done';
      setPill('done');
      pushLog('warshad', `run #${sim.runNo} shipped · spent ${money(sim.spent)}`, 'ok');
      sound.done();
    } else {
      sim.stage += 1;
      sim.progress = 0;
      sim.marks.clear();
      sim.lanes = [];
      sim.status = 'paused';
      setPill('paused');
      const first = STAGES[sim.stage].logs[0];
      pushLog(first[0], fillTemplate(first[1]));
      sound.stage();
    }
    trackWrap.classList.remove('is-running');
    setNote(noteForStatus());
    updatePlayLabel();
    renderStage();
    renderLanes();
  });

  $('#resetBtn').addEventListener('click', () => {
    resetSim(true);
    sound.blip(420, 0.07, 0.035, 'triangle');
  });

  $('#approveBtn').addEventListener('click', () => {
    pushLog('owner', 'gate approved by owner', 'ok');
    sound.approve();
    advance();
  });

  $('#holdBtn').addEventListener('click', () => {
    sim.status = 'paused';
    decision.hidden = true;
    setPill('paused');
    setNote(bi({ ar: 'أوقفتَ التشغيل عند البوابة. اضغط تشغيل عندما تجهز.', en: 'You held the run at the gate. Press play when you are ready.' }));
    updatePlayLabel();
    renderStage();
    pushLog('owner', 'gate held · run paused', 'wait');
    toast(lang() === 'ar' ? 'التشغيل محفوظ عند البوابة.' : 'The run is saved at the gate.', 'info');
  });

  function bindRange(id, out, fmt, apply) {
    const el = $(id);
    const paint = () => {
      const pct = ((el.value - el.min) / (el.max - el.min)) * 100;
      el.style.setProperty('--fill', `${pct}%`);
      if (out) $(out).textContent = fmt(Number(el.value));
    };
    el.addEventListener('input', () => {
      paint();
      apply(Number(el.value));
    });
    paint();
    return el;
  }

  bindRange('#ctlSpeed', '#outSpeed', (v) => `${v.toFixed(2).replace(/0$/, '')}×`, (v) => {
    sim.speed = v;
  });
  bindRange('#ctlBudget', '#outBudget', (v) => `$${v}`, (v) => {
    sim.cap = v;
    $('#cap').textContent = `$${v}`;
    if (sim.status === 'capped' && sim.spent < v) {
      sim.status = 'paused';
      setPill('paused');
      setNote(noteForStatus());
      updatePlayLabel();
      pushLog('budget', `cap raised to $${v} · run may resume`, 'key');
    }
  });
  bindRange('#ctlGlow', '#outGlow', (v) => `${v}%`, (v) => {
    trackWrap.style.setProperty('--glow', String(v / 100));
    $('#factoryRoot').style.setProperty('--glow', String(v / 100));
  });
  bindRange('#ctlParallel', '#outParallel', (v) => String(v), (v) => {
    sim.parallel = v;
    sim.lanes = [];
    renderLanes();
    if (sim.stage === 2) pushLog('architect', `coder lanes set to ${v}`, 'key');
  });

  $('#ctlScenario').addEventListener('change', (e) => {
    sim.scenario = Number(e.target.value);
    sim.custom = '';
    idea.value = lang() === 'ar' ? SCENARIOS[sim.scenario].ideaAr : SCENARIOS[sim.scenario].ideaEn;
    idea.dispatchEvent(new Event('input'));
    resetSim(true);
    renderBench();
    startSim();
  });

  $('#ctlAuto').addEventListener('change', (e) => {
    sim.autoApprove = e.target.checked;
    pushLog('policy', `auto-approve ${sim.autoApprove ? 'on' : 'off'}`, 'key');
    if (sim.autoApprove && sim.status === 'waiting') {
      pushLog('owner', 'gate auto-approved · stage complete', 'ok');
      advance();
    }
    toast(
      sim.autoApprove
        ? bi({ ar: 'الموافقة التلقائية مفعّلة — محاكاة فقط.', en: 'Auto-approve is on — simulation only.' })
        : bi({ ar: 'البوابات تنتظرك مجددًا.', en: 'The gates wait for you again.' }),
      sim.autoApprove ? 'warn' : 'info',
    );
  });

  $('#ctlLog').addEventListener('change', (e) => {
    machineLog.hidden = !e.target.checked;
    $('.stage-machine').classList.toggle('no-log', !e.target.checked);
  });

  $('#ctlGrid').addEventListener('change', (e) => {
    trackWrap.dataset.grid = e.target.checked ? 'on' : 'off';
  });
  trackWrap.dataset.grid = 'on';

  const densitySeg = $('#densitySeg');
  densitySeg.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-density-choice]');
    if (!btn) return;
    $$('button', densitySeg).forEach((b) => b.setAttribute('aria-checked', String(b === btn)));
    $('#factoryRoot').dataset.density = btn.dataset.densityChoice;
    measureConduit();
  });

  $('#randomBtn').addEventListener('click', () => {
    const pick = (id, v) => {
      const el = $(id);
      el.value = String(v);
      el.dispatchEvent(new Event('input'));
    };
    const s = Math.floor(Math.random() * SCENARIOS.length);
    $('#ctlScenario').value = String(s);
    sim.scenario = s;
    sim.custom = '';
    idea.value = lang() === 'ar' ? SCENARIOS[s].ideaAr : SCENARIOS[s].ideaEn;
    idea.dispatchEvent(new Event('input'));
    pick('#ctlSpeed', [0.5, 1, 1.5, 2][Math.floor(Math.random() * 4)]);
    pick('#ctlParallel', 1 + Math.floor(Math.random() * 6));
    pick('#ctlBudget', 5 + Math.floor(Math.random() * 20));
    pick('#ctlGlow', 20 + Math.floor(Math.random() * 17) * 5);
    resetSim(true);
    renderBench();
    startSim();
    toast(bi({ ar: 'إعدادات عشوائية جديدة. التشغيل بدأ.', en: 'New random settings. The run has started.' }), 'info');
  });

  $('#exportBtn').addEventListener('click', () => {
    const payload = {
      generator: 'warsha-website-sim/v3',
      run: sim.runNo,
      scenario: SCENARIOS[sim.scenario].en,
      request: sim.custom || bi(SCENARIOS[sim.scenario]),
      status: sim.status,
      stage: STAGES[sim.stage].tech,
      spent: Number(sim.spent.toFixed(2)),
      cap: sim.cap,
      speed: sim.speed,
      parallelCoders: sim.parallel,
      autoApprove: sim.autoApprove,
      events: sim.log,
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `warsha-run-${sim.runNo}.json`;
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    toast(bi({ ar: 'نُزِّل سجل التشغيل بصيغة JSON.', en: 'The run log was downloaded as JSON.' }), 'ok');
  });

  const soundBtn = $('#soundBtn');
  function syncSoundBtn() {
    soundBtn.setAttribute('aria-pressed', String(sound.on));
    soundBtn.setAttribute('aria-label', sound.on ? (lang() === 'ar' ? 'كتم الصوت' : 'Mute sound') : lang() === 'ar' ? 'تشغيل الصوت' : 'Unmute sound');
    $('#soundUse').setAttribute('href', sound.on ? '#i-sound' : '#i-mute');
  }
  soundBtn.addEventListener('click', () => {
    sound.on = !sound.on;
    store.set('warsha.sound', sound.on ? 'on' : 'off');
    syncSoundBtn();
    if (sound.on) sound.blip(720, 0.07, 0.04, 'triangle');
  });

  nodes.forEach((n) => {
    n.addEventListener('click', () => {
      sim.stage = Number(n.dataset.step);
      sim.progress = 0;
      sim.marks.clear();
      sim.lanes = [];
      if (sim.status === 'done' || sim.status === 'idle') sim.status = 'paused';
      decision.hidden = true;
      if (sim.status === 'waiting') sim.status = 'paused';
      trackWrap.classList.remove('is-running');
      setPill(sim.status === 'running' ? 'running' : 'paused');
      if (sim.status === 'running') trackWrap.classList.add('is-running');
      setNote(noteForStatus());
      updatePlayLabel();
      renderStage();
      renderLanes();
      const first = STAGES[sim.stage].logs[0];
      pushLog(first[0], fillTemplate(first[1]), 'key');
      sound.blip(500, 0.05, 0.03, 'triangle');
    });
  });

  $('#track').addEventListener('keydown', (e) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    const i = nodes.indexOf(document.activeElement);
    if (i < 0) return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    const next =
      e.key === 'Home' ? 0 : e.key === 'End' ? nodes.length - 1 : (i + (root.dir === 'rtl' ? -dir : dir) + nodes.length) % nodes.length;
    nodes[next].focus();
    nodes[next].click();
  });

  function measureConduit() {
    const c = $('.conduit');
    if (c) trackWrap.style.setProperty('--conduit-w', `${c.getBoundingClientRect().width}px`);
  }
  addEventListener('resize', measureConduit);
  if (window.ResizeObserver) new ResizeObserver(measureConduit).observe(trackWrap);

  /* ── gate demo ──────────────────────────────────────────────── */
  const gateActs = $('#gateActs');
  const gateChanges = $('#gateChanges');
  const gateResult = $('#gateResult');
  const gateNote = $('#gateNote');
  const gateNoteError = $('#gateNoteError');
  const rejectDialog = $('#rejectDialog');

  function gateSay(text, kind) {
    gateResult.hidden = false;
    gateResult.className = `gate-result is-${kind}`;
    gateResult.innerHTML = `${icon(kind === 'ok' ? 'check' : kind === 'bad' ? 'x' : 'info')}<span></span>`;
    gateResult.lastElementChild.textContent = text;
  }

  gateActs.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-gate]');
    if (!btn) return;
    const act = btn.dataset.gate;
    if (act === 'approve') {
      gateChanges.hidden = true;
      gateSay(
        bi({ ar: 'اعتُمد التصميم. المبرمجون بدأوا العمل على الشاشات، والتشغيل انتقل إلى مرحلة البناء.', en: 'Design approved. Coders started on the screens and the run moved to the build stage.' }),
        'ok',
      );
      sound.approve();
      toast(bi({ ar: 'وافقتَ على البوابة. الفريق يكمل.', en: 'You approved the gate. The team continues.' }), 'ok');
    } else if (act === 'changes') {
      gateChanges.hidden = false;
      gateResult.hidden = true;
      gateNote.focus();
      sound.blip(480, 0.06, 0.03, 'triangle');
    } else {
      if (typeof rejectDialog.showModal === 'function') rejectDialog.showModal();
      else {
        gateSay(bi({ ar: 'رُفض التصميم وتوقّف التشغيل. كل ما أُنجز محفوظ.', en: 'Design rejected and the run stopped. Everything produced is saved.' }), 'bad');
      }
      sound.stop();
    }
  });

  gateChanges.addEventListener('submit', (e) => {
    e.preventDefault();
    if (!gateNote.value.trim()) {
      gateNoteError.hidden = false;
      gateNote.setAttribute('aria-invalid', 'true');
      gateNote.focus();
      sound.stop();
      return;
    }
    gateNoteError.hidden = true;
    gateNote.removeAttribute('aria-invalid');
    gateChanges.hidden = true;
    gateSay(
      bi({ ar: 'وصل تعديلك إلى المصمّم. عاد الفريق إلى مرحلة التصميم ولن يتجاوزها قبل موافقتك الجديدة.', en: 'Your change reached the designer. The team returned to the design stage and will not cross it without a new approval.' }),
      'info',
    );
    toast(bi({ ar: 'أُرسل طلب التعديل إلى الفريق.', en: 'The change request was sent to the team.' }), 'info');
    gateNote.value = '';
    sound.approve();
  });

  $('#gateCancel').addEventListener('click', () => {
    gateChanges.hidden = true;
    gateNoteError.hidden = true;
    gateNote.value = '';
  });

  $('#rejectConfirm').addEventListener('click', () => {
    rejectDialog.close();
    gateChanges.hidden = true;
    gateSay(bi({ ar: 'رُفض التصميم وتوقّف التشغيل #3. كل ما أُنجز محفوظ ولم تُصرف ميزانية إضافية.', en: 'Design rejected and run #3 stopped. Everything produced is saved and no further budget was spent.' }), 'bad');
    toast(bi({ ar: 'توقّف التشغيل عند البوابة.', en: 'The run stopped at the gate.' }), 'bad');
  });
  $('#rejectCancel').addEventListener('click', () => rejectDialog.close());

  /* ── modes + bench tabs ─────────────────────────────────────── */
  const modeSeg = $('#modeSeg');
  modeSeg.addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-mode]');
    if (!btn) return;
    setMode(btn.dataset.mode);
  });
  modeSeg.addEventListener('keydown', (e) => {
    if (!['ArrowLeft', 'ArrowRight'].includes(e.key)) return;
    const btns = $$('button', modeSeg);
    const i = btns.indexOf(document.activeElement);
    if (i < 0) return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    const next = btns[(i + (root.dir === 'rtl' ? -dir : dir) + btns.length) % btns.length];
    next.focus();
    setMode(next.dataset.mode);
  });

  function setMode(mode) {
    $$('button', modeSeg).forEach((b) => b.setAttribute('aria-checked', String(b.dataset.mode === mode)));
    const advanced = mode === 'advanced';
    $('#viewSimple').hidden = advanced;
    $('#viewAdvanced').hidden = !advanced;
    $('#appFrame').dataset.density = advanced ? 'compact' : 'comfortable';
    sound.blip(advanced ? 600 : 500, 0.05, 0.03, 'triangle');
    if (advanced) renderBench();
  }

  const benchTabs = $('#benchTabs');
  const tabBtns = $$('button', benchTabs);
  function selectTab(btn) {
    tabBtns.forEach((b) => {
      const on = b === btn;
      b.setAttribute('aria-selected', String(on));
      b.tabIndex = on ? 0 : -1;
      $(`#${b.getAttribute('aria-controls')}`).hidden = !on;
    });
    if (btn.dataset.tab === 'cost') {
      $$('#costChart .cost-track i').forEach((el) => {
        el.style.inlineSize = '0%';
        requestAnimationFrame(() => {
          el.style.inlineSize = `${el.dataset.w}%`;
        });
      });
    }
  }
  benchTabs.addEventListener('click', (e) => {
    const btn = e.target.closest('button[role="tab"]');
    if (!btn) return;
    selectTab(btn);
    sound.blip(560, 0.04, 0.025, 'triangle');
  });
  benchTabs.addEventListener('keydown', (e) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    const i = tabBtns.indexOf(document.activeElement);
    if (i < 0) return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    const next =
      e.key === 'Home' ? tabBtns[0] : e.key === 'End' ? tabBtns[tabBtns.length - 1] : tabBtns[(i + (root.dir === 'rtl' ? -dir : dir) + tabBtns.length) % tabBtns.length];
    next.focus();
    selectTab(next);
  });

  $$('.rail-row').forEach((row) => {
    row.addEventListener('click', () => {
      $$('.rail-row').forEach((r) => r.classList.toggle('is-sel', r === row));
      toast(
        bi({ ar: `فتحتَ التشغيل #${row.dataset.run}.`, en: `Opened run #${row.dataset.run}.` }),
        'info',
      );
      renderBench();
    });
  });

  /* ── boot ───────────────────────────────────────────────────── */
  renderTeam();
  renderRoad();
  renderFaq();
  renderBench();
  syncSoundBtn();
  resetSim(false);
  measureConduit();
  trackWrap.style.setProperty('--glow', '0.6');
  $('#factoryRoot').style.setProperty('--glow', '0.6');
  applyLang(lang());
  requestAnimationFrame(loop);

  // The factory starts itself the first time it scrolls into view.
  const factoryIO = new IntersectionObserver(
    (entries) => {
      entries.forEach((en) => {
        if (!en.isIntersecting) return;
        factoryIO.unobserve(en.target);
        if (sim.status === 'idle' && !reduced()) startSim();
      });
    },
    { threshold: 0.25 },
  );
  factoryIO.observe($('#factoryRoot'));

  document.addEventListener('pointerdown', () => sound.arm(), { once: true });
})();