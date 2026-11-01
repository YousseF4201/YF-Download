<div dir="rtl" align="right">

# ADR-005: استراتيجية التعريب والترجمة (i18n)

**الحالة:** مقترح — ينتظر الموافقة
**التاريخ:** نوفمبر 2026

## السياق

المشروع يدعم العربية أولًا والإنجليزية بضغطة زر، مع إمكانية إضافة لغات مستقبلًا.
نحتاج لاستراتيجية ترجمة تدعم:
- **RTL** (من اليمين لليسار) للعربية.
- **LTR** (من اليسار لليمين) للإنجليزية.
- تبديل اللغة دون إعادة تشغيل التطبيق.
- إضافة لغات جديدة بسهولة.

## القرار

### ملفات JSON للترجمة
- كل لغة في ملف JSON منفصل: `ar.json`، `en.json`.
- المفاتيح بالإنجليزية، والقيم بلغة الملف.
- الملفات تُوضع في `src/lib/i18n/`.

### مثال:

**`ar.json`**:
```json
{
  "app.title": "YF Download",
  "download.add": "تحميل جديد",
  "download.pause": "إيقاف مؤقت",
  "download.resume": "استئناف",
  "download.speed": "السرعة",
  "download.remaining": "الوقت المتبقي",
  "settings.language": "اللغة",
  "settings.save_path": "مجلد الحفظ"
}
```

**`en.json`**:
```json
{
  "app.title": "YF Download",
  "download.add": "New Download",
  "download.pause": "Pause",
  "download.resume": "Resume",
  "download.speed": "Speed",
  "download.remaining": "Time Remaining",
  "settings.language": "Language",
  "settings.save_path": "Save Folder"
}
```

### اكتشاف اللغة
1. إذا حدد المستخدم لغة في الإعدادات → تُستخدم.
2. وإلا، يُكتشف لغة النظام (`navigator.language` أو ما يعادلها في Tauri).
3. إذا كانت لغة النظام عربية → العربية. وإلا → العربية أيضًا (العربية هي الافتراضية).

### دعم RTL
- العنصر `<html>` يحصل على `dir="rtl"` أو `dir="ltr"` حسب اللغة.
- CSS يستخدم الخصائص المنطقية (`margin-inline-start` بدلًا من `margin-left`).
- لا يحتاج تخطيطين منفصلين — CSS Logical Properties + Flexbox يتعاملان مع RTL تلقائيًا.

### المكتبة المقترحة
- **`svelte-i18n`** أو **`typesafe-i18n`** — سيُختار عند بدء المرحلة 4.

> ⚠️ تفاصيل التنفيذ لم تُختبر بعد.

## العواقب

- كل نص ظاهر للمستخدم يجب أن يمر عبر نظام الترجمة (لا نصوص مكتوبة مباشرة في HTML).
- عند إضافة ميزة جديدة، يجب إضافة النصوص في كلا الملفين (ar.json وen.json).
- CSS يجب أن يتجنب `left`/`right` ويستخدم `start`/`end` (logical properties).

</div>
