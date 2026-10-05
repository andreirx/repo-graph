# D-JSAH-CURSOR-1 — the constructor cursors `explain <Type>` prints are stable keys, on every language

Raised: 2026-10-05 by the operator while root-causing JAVA-SYMBOL-AMBIGUITY-HINT-1.

**The problem in plain words.** When a bare name resolves to a type over its constructors, `explain` prints `N constructor(s) also match: explain '<cursor>'` so the constructor query is never hidden (CPP-DECLARATORS-1 §2.3). Today the cursor is the constructor's QUALIFIED NAME (`explain 'Widget::Widget'`; test `render_constructor_counted_line_reports_omitted_total` pins `3 constructors also match: explain 'Widget::Widget'`). A class with more than one constructor has several constructors with that same qualified name, so the printed cursor is itself ambiguous — it does not run as printed (RG-REQ-012-L03). For Java the qualified name would be `<pkg>.<Type>.<init>`, shared by all constructors of the type (kafka `KafkaProducer`: six).

**Options (reward / risk).**
- A — the cursor is the constructor's stable key (the full key `find` prints: `<repo_uid>:<path>#<name>:SYMBOL:CONSTRUCTOR[:dupN]`), on C++ and Java alike; one cursor per constructor, each runnable. Reward: every printed cursor runs (L03); one cursor form across `find`, the ambiguity listing and this line. Risk: longer lines; the C++ render tests' expected strings change (renamed in the packet).
- B — the qualified name when the type has exactly one constructor, the stable key otherwise. Reward: short cursor in the common case. Risk: two cursor forms for one line; the reader must know which one they got.
- C — keep the qualified name. Risk: an unrunnable cursor whenever a class has several constructors (L03 violated today on C++).

**Resolved: 2026-10-05 by the OPERATOR (in-place manager) as A**; the human may override.
