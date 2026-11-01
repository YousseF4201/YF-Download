# سجل التغييرات — CHANGELOG.md

جميع التغييرات الملحوظة في مشروع **YF Download** سيتم توثيقها في هذا الملف.

التنسيق مبني على [Keep a Changelog](https://keepachangelog.com/ar/1.0.0/)،
وهذا المشروع يلتزم بـ [Semantic Versioning](https://semver.org/lang/ar/).

---

## [0.1.0] - 2026-11-01

### تمت إضافته (Added)
- **ملفات التوثيق والأساس (Foundation Docs)**:
  - `README.md`: نظرة عامة على المشروع وأهدافه.
  - `docs/PRD.md`: وثيقة متطلبات المنتج والميزات.
  - `docs/ARCHITECTURE.md`: المخطط المعماري للنواة والواجهة والجسر.
  - `docs/adr/`: وثائق القرارات المعمارية (001 إلى 006).
  - `docs/ROADMAP.md`: خريطة الطريق التفصيلية لكافة المراحل من 0 إلى 7.
  - `AGENTS.md`: ميثاق وقواعد العمل الصارمة لفرق التطوير والوكلاء.
- **إطار العمل والتطوير (Skeleton & Architecture)**:
  - تهيئة هيكل تطبيق Tauri v2 مع واجهة Svelte 5 و Vite و TypeScript.
  - إعداد النواة الخلفية بـ Rust (`src-tauri`).
  - إضافة أمر Tauri Command تجريبي (`greet`) لاختبار جسر الـ IPC.
  - إضافة اختبارات الوحدة في Rust للتأكد من استجابة دالة التحية.
- **أدوات الجودة والتكامل المستمر (Quality & CI/CD)**:
  - إعداد مسار GitHub Actions (`ci.yml`) للفحص، التنسيق، الاختبار والبناء على Ubuntu و Windows.
  - تهيئة أدوات الفحص والتنسيق: `ESLint`، `Prettier`، `cargo fmt`، و `cargo clippy`.
  - إضافة سكربتات إدارة الجودة إلى `package.json`.
