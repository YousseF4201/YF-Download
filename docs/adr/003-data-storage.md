<div dir="rtl" align="right">

# ADR-003: طريقة حفظ البيانات

**الحالة:** مقترح — ينتظر الموافقة
**التاريخ:** نوفمبر 2026

## السياق

نحتاج لحفظ:
- معلومات التحميلات (الرابط، الاسم، الحجم، الحالة، التقدم).
- حالة الأجزاء (segments) لكل تحميل (للاستئناف).
- إعدادات المستخدم.
- سجل التحميلات المكتملة.

يجب أن يكون الحل مضمنًا (لا خادم قاعدة بيانات منفصل)، سريعًا، وموثوقًا.

## الخيارات

### 1. SQLite ✅ (الاختيار)
| الجانب | التقييم |
|--------|---------|
| الأداء | ممتاز للقراءة والكتابة المحلية |
| الموثوقية | ممتاز — ACID transactions |
| الاستعلامات | ممتاز — SQL كامل |
| الحجم | لا يُذكر — مكتبة مضمنة |
| مكتبة Rust | `rusqlite` — ناضجة ومستقرة |
| النقل | ملف واحد — سهل النسخ الاحتياطي |

### 2. ملفات JSON
| الجانب | التقييم |
|--------|---------|
| البساطة | ممتاز — سهل القراءة والكتابة |
| الموثوقية | ضعيف — لا ذرية (atomic)، تلف محتمل عند انقطاع الكتابة |
| الاستعلامات | ضعيف — لا فهرسة، بحث خطي |
| مع كثرة التحميلات | يبطئ — كتابة الملف كاملًا عند كل تحديث |

### 3. sled (قاعدة بيانات Rust مضمنة)
| الجانب | التقييم |
|--------|---------|
| الأداء | جيد |
| النضج | متوسط — لم يصل 1.0 بعد |
| الاستعلامات | محدود — key-value فقط |

## القرار

**SQLite** (عبر `rusqlite`) — لأنه:
1. **ACID transactions** — لا تلف في البيانات حتى عند انقطاع الكهرباء.
2. **SQL كامل** — استعلامات مرنة (مثلًا: "أعطني كل التحميلات غير المكتملة مرتبة بالأولوية").
3. **ملف واحد** — سهل النقل والنسخ الاحتياطي.
4. **ناضج ومُختبَر** — يُستخدم في مليارات التطبيقات.
5. **`rusqlite` مستقرة** — أكثر مكتبة SQLite نضجًا في Rust.

## الهيكل المبدئي للجداول

```sql
-- التحميلات
CREATE TABLE downloads (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    url         TEXT NOT NULL,
    file_name   TEXT NOT NULL,
    save_path   TEXT NOT NULL,
    total_size  INTEGER,            -- بالبايت، NULL إذا غير معروف
    status      TEXT NOT NULL,       -- pending, active, paused, completed, failed
    category    TEXT,
    priority    INTEGER DEFAULT 0,
    created_at  TEXT NOT NULL,
    completed_at TEXT,
    error_msg   TEXT
);

-- أجزاء التحميل (للاستئناف)
CREATE TABLE segments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    download_id INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    index_num   INTEGER NOT NULL,    -- ترتيب الجزء
    start_byte  INTEGER NOT NULL,
    end_byte    INTEGER NOT NULL,
    downloaded  INTEGER DEFAULT 0,   -- كم بايت حُمِّل من هذا الجزء
    status      TEXT NOT NULL,       -- pending, active, completed, failed
    temp_path   TEXT                 -- مسار الملف المؤقت لهذا الجزء
);

-- الإعدادات
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

> ⚠️ هذا الهيكل أوّلي ولم يُختبر. سيُحدَّث عند كتابة الكود.

## العواقب

- يحتاج مكتبة `rusqlite` (تبعية خارجية، لكنها ناضجة ومتوافقة مع MIT).
- يجب إدارة هجرات قاعدة البيانات (migrations) عند تغيير الهيكل.
- على لينكس، SQLite متوفرة غالبًا. على ويندوز، `rusqlite` تُجمِّعها مع التطبيق (بندلة).

</div>
