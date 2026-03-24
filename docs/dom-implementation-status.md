# DOM Implementation Status

> Ärlig inventering: vad är Rust-native vs JS-polyfill.

## Princip

- **Native (Rust)** = Implementerad i `dom_bridge.rs`, läser/skriver ArenaDom direkt. Riktig implementation.
- **Polyfill (JS)** = Shim i `wpt/polyfills.js`. Fungerar för WPT-validering men är **inte** riktig implementation. Mål: migrera till Rust.
- **Stub** = Returnerar hårdkodat värde. Ingen riktig logik.

## WPT Score

| Suite | Cases | Passed | Rate | Datum |
|-------|-------|--------|------|-------|
| dom/ | 2,004 | 1,324 | 66.1% | 2026-03-24 |

**OBS:** En del av pass-raten beror på polyfills. Riktig native-only score är lägre.

---

## Document Methods

| Method | Status | Impl |
|--------|--------|------|
| `getElementById(id)` | **Native** | Rust — rekursiv sökning i ArenaDom |
| `querySelector(sel)` | **Native** | Rust — full CSS selector matching |
| `querySelectorAll(sel)` | **Native** | Rust — returnerar alla matchande noder |
| `createElement(tag)` | **Native** | Rust — skapar ny nod i ArenaDom |
| `createTextNode(text)` | **Native** | Rust — skapar Text-nod |
| `createComment(text)` | **Native** | Rust — skapar Comment-nod |
| `createDocumentFragment()` | **Native** | Rust — skapar fragment |
| `getElementsByClassName(cls)` | **Native** | Rust — rekursiv sökning |
| `getElementsByTagName(tag)` | **Native** | Rust — rekursiv sökning |
| `createElementNS(ns, qname)` | Polyfill | JS — delegerar till createElement |
| `getElementsByTagNameNS(ns, tag)` | Polyfill | JS — ignorerar namespace |
| `createEvent(type)` | Polyfill | JS — skapar Event-objekt med rätt typ |
| `document.implementation` | Polyfill | JS — createDocument/createHTMLDocument |
| `document.title` | Polyfill | JS — getter/setter via `<title>` element |
| `document.URL` | Polyfill | JS — alias för location.href |

## Element Methods — Native (Rust)

| Method | Detaljer |
|--------|---------|
| `getAttribute(name)` | Läser attribut, lowercasar namn |
| `setAttribute(name, value)` | Skriver attribut till ArenaDom |
| `removeAttribute(name)` | Tar bort attribut |
| `hasAttribute(name)` | Kontrollerar existens |
| `getAttributeNames()` | Returnerar alla attributnamn |
| `textContent` (get/set) | Accessor — rekursiv text extraction/replacement |
| `innerHTML` (get/set) | Accessor — serialiserar/parsar HTML |
| `outerHTML` (get) | Serialiserar element + barn |
| `insertAdjacentHTML(pos, html)` | Parsar och infogar HTML |
| `appendChild(child)` | Flyttar nod i ArenaDom |
| `removeChild(child)` | Tar bort, kastar NotFoundError |
| `insertBefore(new, ref)` | Index-baserad insertion |
| `replaceChild(new, old)` | Ersätter nod, kastar TypeError/NotFoundError |
| `cloneNode(deep)` | Rekursiv djup kopia i ArenaDom |
| `parentNode` / `childNodes` / `children` | Traversering via ArenaDom |
| `firstChild` / `lastChild` / `nextSibling` / `previousSibling` | Navigering |
| `firstElementChild` / `lastElementChild` / `nextElementSibling` | Element-filtrering |
| `closest(selector)` | Traverserar ancestors med CSS matching |
| `matches(selector)` | Testar CSS selector-matchning |
| `contains(other)` | Rekursiv descendant-check |
| `isConnected` | Parent-chain till document |
| `getRootNode()` | Traverserar till rot |
| `classList.add/remove/toggle/contains/replace` | Modifierar class-attribut |
| `style.setProperty/getPropertyValue/removeProperty` | Inline style manipulation |
| `addEventListener/removeEventListener/dispatchEvent` | Event-system med bubbling |
| `focus()` / `blur()` / `click()` | Focus-tracking + event dispatch |
| `getBoundingClientRect()` | Estimerad rect från tag+style |
| `id` / `className` | Polyfill-accessor som skriver till setAttribute |

## Element Methods — Polyfill (JS, behöver Rust-migration)

| Method | Prioritet | Kommentar |
|--------|-----------|----------|
| ~~`remove()`~~ | ~~Hög~~ | **Migrerad till Rust** (2026-03-24) |
| ~~`before()` / `after()`~~ | ~~Hög~~ | **Migrerad till Rust** (2026-03-24) |
| ~~`replaceWith()`~~ | ~~Hög~~ | **Migrerad till Rust** (2026-03-24) |
| `prepend()` / `append()` / `replaceChildren()` | Hög | ParentNode convenience |
| ~~`toggleAttribute(name, force)`~~ | ~~Medium~~ | **Migrerad till Rust** (2026-03-24) |
| `insertAdjacentElement(pos, el)` | Medium | Wrapper kring insertBefore/appendChild |
| `insertAdjacentText(pos, text)` | Medium | Wrapper kring createTextNode + insert |
| `setAttributeNS(ns, qname, val)` | Medium | Ignorerar namespace nu — behöver riktig NS-stöd |
| `getAttributeNS(ns, local)` | Medium | Samma |
| `hasAttributeNS(ns, local)` | Medium | Samma |
| `removeAttributeNS(ns, local)` | Medium | Samma |
| `getAttributeNode(name)` | Låg | Returnerar Attr-liknande objekt |
| `getAttributeNodeNS(ns, local)` | Låg | Samma |
| `moveBefore(node, child)` | Låg | Ny spec — delegerar till insertBefore |
| `lookupNamespaceURI(prefix)` | Låg | Traverserar parent-chain |
| `lookupPrefix(ns)` | Låg | Samma |
| `isDefaultNamespace(ns)` | Låg | Wrapper |

## CharacterData — Polyfill (JS, behöver Rust-migration)

| Method | Prioritet |
|--------|-----------|
| `.data` (get/set) | **Hög** — alias för textContent, men korrekt null→"" hantering |
| `.length` | Hög |
| `.substringData(offset, count)` | Hög |
| `.appendData(data)` | Hög |
| `.insertData(offset, data)` | Hög |
| `.deleteData(offset, count)` | Hög |
| `.replaceData(offset, count, data)` | Hög |

## element.attributes (NamedNodeMap) — Polyfill

Returnerar nytt array-objekt vid varje anrop. **Inte live** som spec kräver.
Behöver Rust-native NamedNodeMap med:
- Live-koppling till ArenaDom
- `.length`, `.item(i)`, `[i]`-access
- `.getNamedItem(name)`, `.getNamedItemNS(ns, name)`
- `.setNamedItem(attr)`, `.removeNamedItem(name)`

## Prototypkedja — Polyfill

`Object.create(HTMLDivElement.prototype)` i `make_element_object`. Fungerar för `instanceof` men:
- Konstruktorer är tomma funktioner (inga properties)
- Ingen riktig WebIDL-kompatibilitet
- `typeof HTMLElement === "function"` men det är en stub-funktion

## Event-system — Delvis native

| Del | Status |
|-----|--------|
| `addEventListener/removeEventListener` | **Native** |
| `dispatchEvent` med bubbling | **Native** |
| `Event` / `CustomEvent` konstruktorer | **Native** (Rust) |
| `MouseEvent`, `KeyboardEvent` etc. | Polyfill (tomma konstruktorer) |
| `document.createEvent(type)` | Polyfill |
| `event.initEvent()` | Polyfill |
| `event.target/currentTarget` | Delvis (target=null vid skapande) |
| Capture phase | Inte implementerad |
| `stopPropagation` / `stopImmediatePropagation` | Native (flagga sätts, bubbling stoppas) |

## CSS Selector Parser — Native (Rust)

| Feature | Status |
|---------|--------|
| ID (`#id`) | Native |
| Class (`.cls`) | Native |
| Tag (`div`) | Native |
| Attribute (`[attr]`, `[attr="val"]`, `[attr~="val"]`) | Native |
| Child (`>`) / Descendant (` `) | Native |
| `:first-child`, `:last-child`, `:nth-child`, `:nth-of-type` | Native |
| `:root`, `:empty`, `:checked`, `:disabled`, `:enabled`, `:focus` | Native |
| `:not(sel)` | Native |
| Comma-separated | Native |
| CSS escape (`\30 foo`) | Native (css_unescape) |
| `:has()`, `:is()`, `:where()` | Saknas |
| `+` (adjacent sibling), `~` (general sibling) | Saknas |

---

## Migrationsplan: Polyfill → Rust

### Fas 1 (Hög prioritet — vanligaste API:erna)
1. `element.remove()` — trivial i Rust
2. `before()` / `after()` / `replaceWith()` — ChildNode i Rust
3. `prepend()` / `append()` / `replaceChildren()` — ParentNode i Rust
4. CharacterData `.data`, `.substringData()`, etc. — direkt på text/comment-noder
5. `toggleAttribute()` — enkel logik

### Fas 2 (Medium — namespace-stöd)
6. `setAttributeNS` / `getAttributeNS` / `hasAttributeNS` / `removeAttributeNS`
7. `element.attributes` som riktig NamedNodeMap
8. `createElementNS`

### Fas 3 (Event-förbättringar)
9. `MouseEvent`, `KeyboardEvent` etc. som riktiga typer
10. `createEvent()` i Rust
11. Capture phase i event dispatch

### Fas 4 (CSS selectors)
12. `+` och `~` combinators
13. `:has()`, `:is()`, `:where()` pseudo-classes
