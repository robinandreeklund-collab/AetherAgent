/*
 * AetherAgent WPT Polyfills
 *
 * Fyller luckor i DOM bridge som WPT-tester förväntar sig.
 * Laddas före testharness.js.
 */

// ─── CharacterData: .data, .length, .substringData, .replaceData, etc. ──────
// Text (nodeType=3) och Comment (nodeType=8) måste ha CharacterData-metoder
(function() {
  if (typeof document === 'undefined') return;

  function patchCharacterData(node) {
    if (!node || typeof node !== 'object') return node;
    var nt = node.nodeType;
    if (nt !== 3 && nt !== 8) return node;

    // .data getter/setter — alias för textContent (spec: null → "", undefined → "undefined")
    if (!('data' in node)) {
      Object.defineProperty(node, 'data', {
        get: function() { return this.textContent || ''; },
        set: function(val) {
          this.textContent = (val === null) ? '' : String(val);
        },
        configurable: true
      });
    }

    // .length
    if (!('length' in node)) {
      Object.defineProperty(node, 'length', {
        get: function() { return (this.data || '').length; },
        configurable: true
      });
    }

    // .nodeValue — alias för data
    if (!('nodeValue' in node)) {
      Object.defineProperty(node, 'nodeValue', {
        get: function() { return this.data; },
        set: function(val) { this.data = val; },
        configurable: true
      });
    }

    // CharacterData methods — nu Rust-native med UTF-16 code unit counting

    return node;
  }

  globalThis.__patchCharacterData = patchCharacterData;
})();

// ─── document.implementation ─────────────────────────────────────────────────
// DOMImplementation med createDocument, createHTMLDocument, createDocumentType
(function() {
  if (typeof document === 'undefined') return;
  if (!document.implementation) {
    document.implementation = {};
  }
  var impl = document.implementation;

  if (!impl.createHTMLDocument) {
    impl.createHTMLDocument = function(title) {
      // Bygg en riktig DOM-struktur via vår arena
      var html = document.createElement('html');
      var head = document.createElement('head');
      var body = document.createElement('body');
      html.appendChild(head);
      html.appendChild(body);
      if (title !== undefined) {
        var titleEl = document.createElement('title');
        titleEl.textContent = title || '';
        head.appendChild(titleEl);
      }
      // Skapa ett dokument-liknande objekt med delegering till riktiga element
      var doc = document.createDocumentFragment();
      doc.appendChild(html);
      // Lägg till document-liknande egenskaper
      doc.nodeType = 9;
      doc.nodeName = '#document';
      doc.documentElement = html;
      doc.head = head;
      doc.body = body;
      doc.title = title || '';
      doc.implementation = document.implementation;
      doc.createElement = document.createElement.bind(document);
      doc.createTextNode = document.createTextNode.bind(document);
      doc.createComment = document.createComment.bind(document);
      doc.createDocumentFragment = document.createDocumentFragment.bind(document);
      doc.createElementNS = document.createElementNS ? document.createElementNS.bind(document) : undefined;
      // Query-metoder söker i detta dokumentets träd
      doc.getElementById = function(id) {
        // Rekursiv sökning i hela dokumentträdet
        function findById(node, target) {
          if (node.id === target) return node;
          var kids = node.childNodes || [];
          for (var i = 0; i < kids.length; i++) {
            var found = findById(kids[i], target);
            if (found) return found;
          }
          return null;
        }
        return findById(html, id);
      };
      doc.querySelector = function(sel) { return html.querySelector(sel); };
      doc.querySelectorAll = function(sel) { return html.querySelectorAll(sel); };
      doc.getElementsByTagName = function(tag) { return html.getElementsByTagName(tag); };
      doc.getElementsByClassName = function(cls) { return html.getElementsByClassName(cls); };
      doc.getElementsByTagNameNS = function(ns, tag) { return html.getElementsByTagNameNS ? html.getElementsByTagNameNS(ns, tag) : []; };
      doc.adoptNode = function(node) { return node; };
      doc.importNode = function(node, deep) { return node.cloneNode(deep); };
      doc.createRange = function() {
        if (typeof Range !== 'undefined') return new Range();
        return document.createRange();
      };
      doc.createTreeWalker = function(root, show, filter) {
        return document.createTreeWalker(root, show, filter);
      };
      doc.createNodeIterator = function(root, show, filter) {
        return document.createNodeIterator(root, show, filter);
      };
      doc.createCDATASection = function(data) {
        var node = document.createComment(data);
        node.nodeType = 4; // CDATA_SECTION_NODE
        node.nodeName = '#cdata-section';
        return node;
      };
      doc.createProcessingInstruction = function(target, data) {
        var node = document.createComment(data);
        node.nodeType = 7;
        node.nodeName = target;
        node.target = target;
        node.data = data;
        return node;
      };
      // Event delegation
      doc.addEventListener = function() {};
      doc.removeEventListener = function() {};
      doc.dispatchEvent = function() { return true; };
      return doc;
    };
  }

  if (!impl.createDocument) {
    impl.createDocument = function(namespace, qualifiedName, doctype) {
      var doc = impl.createHTMLDocument('');
      doc.contentType = namespace ? 'application/xml' : 'text/html';
      return doc;
    };
  }

  if (!impl.createDocumentType) {
    impl.createDocumentType = function(qualifiedName, publicId, systemId) {
      return {
        nodeType: 10,
        nodeName: qualifiedName || '',
        name: qualifiedName || '',
        publicId: publicId || '',
        systemId: systemId || '',
        ownerDocument: document
      };
    };
  }

  if (!impl.hasFeature) {
    impl.hasFeature = function() { return true; };
  }
})();

// ─── document.title ─────────────────────────────────────────────────────────
// Många WPT-tester läser document.title för att identifiera sig.
(function() {
  if (typeof document !== 'undefined' && !('title' in document)) {
    Object.defineProperty(document, 'title', {
      get: function() {
        var el = document.querySelector('title');
        return el ? el.textContent : '';
      },
      set: function(val) {
        var el = document.querySelector('title');
        if (!el) {
          el = document.createElement('title');
          var head = document.head || document.querySelector('head');
          if (head) head.appendChild(el);
        }
        el.textContent = val;
      },
      configurable: true
    });
  }
})();

// ─── document.URL ───────────────────────────────────────────────────────────
(function() {
  if (typeof document !== 'undefined' && !('URL' in document)) {
    Object.defineProperty(document, 'URL', {
      get: function() {
        return (typeof window !== 'undefined' && window.location)
          ? window.location.href
          : 'about:blank';
      },
      configurable: true
    });
  }
})();

// ─── document.location alias ────────────────────────────────────────────────
(function() {
  if (typeof document !== 'undefined' && typeof window !== 'undefined') {
    if (!document.location && window.location) {
      try {
        Object.defineProperty(document, 'location', {
          get: function() { return window.location; },
          configurable: true
        });
      } catch(e) {}
    }
  }
})();

// ─── Event-typ-konstruktorer ─────────────────────────────────────────────────
(function() {
  // Definiera saknade event-typer som ärver från Event
  var eventTypes = [
    'BeforeUnloadEvent', 'CompositionEvent', 'FocusEvent', 'InputEvent',
    'KeyboardEvent', 'MouseEvent', 'UIEvent', 'WheelEvent', 'TouchEvent',
    'AnimationEvent', 'TransitionEvent', 'PointerEvent', 'HashChangeEvent',
    'PopStateEvent', 'StorageEvent', 'PageTransitionEvent', 'ProgressEvent',
    'ClipboardEvent', 'DragEvent', 'ErrorEvent', 'MessageEvent',
    'PromiseRejectionEvent', 'SecurityPolicyViolationEvent',
    'DeviceMotionEvent', 'DeviceOrientationEvent', 'GamepadEvent',
    'MediaQueryListEvent', 'FormDataEvent', 'SubmitEvent'
  ];
  eventTypes.forEach(function(name) {
    if (!globalThis[name]) {
      globalThis[name] = function(type, opts) {
        this.type = type || '';
        this.bubbles = (opts && opts.bubbles) || false;
        this.cancelable = (opts && opts.cancelable) || false;
        this.defaultPrevented = false;
        this.target = null;
        this.srcElement = null;
        this.currentTarget = null;
        this.eventPhase = 0;
        this.timeStamp = Date.now();
        this.isTrusted = false;
        this.composed = false;
        this.detail = (opts && opts.detail) || null;
        this.view = null;
        this.relatedTarget = null;
        this.defaultPrevented = false;
        this.returnValue = true;
        this.preventDefault = function() { if (!this.__passive) { this.defaultPrevented = true; this.returnValue = false; } };
        this.stopPropagation = function() {};
        this.stopImmediatePropagation = function() {};
        this.initEvent = function(t, b, c) { this.type = t; this.bubbles = !!b; this.cancelable = !!c; };
      };
      if (typeof Event !== 'undefined') {
        globalThis[name].prototype = Object.create(Event.prototype);
        globalThis[name].prototype.constructor = globalThis[name];
      }
    }
  });
})();

// ─── document.createEvent() ─────────────────────────────────────────────────
(function() {
  if (typeof document === 'undefined') return;

  // Mappning: case-insensitive alias → konstruktor
  var aliases = {
    'event': Event, 'events': Event, 'htmlevents': Event,
    'customevent': typeof CustomEvent !== 'undefined' ? CustomEvent : Event,
    'uievent': UIEvent, 'uievents': UIEvent,
    'mouseevent': MouseEvent, 'mouseevents': MouseEvent,
    'keyboardevent': KeyboardEvent,
    'compositionevent': CompositionEvent,
    'focusevent': FocusEvent,
    'inputevent': InputEvent,
    'wheelevent': WheelEvent,
    'beforeunloadevent': BeforeUnloadEvent,
    'touchevent': typeof TouchEvent !== 'undefined' ? TouchEvent : null,
    'animationevent': AnimationEvent,
    'transitionevent': TransitionEvent,
    'pointerevent': PointerEvent,
    'hashchangeevent': HashChangeEvent,
    'popstateevent': PopStateEvent,
    'storageevent': StorageEvent,
    'progressevent': ProgressEvent,
    'messageevent': MessageEvent,
    'dragevent': DragEvent,
    'errorevent': ErrorEvent,
    'clipboardevent': ClipboardEvent,
    'submitevent': SubmitEvent,
    'svgevents': Event, 'svgevent': Event,
    'textevent': typeof CompositionEvent !== 'undefined' ? CompositionEvent : Event,
    'mutationevent': Event, 'mutationevents': Event,
    'devicemotionevent': DeviceMotionEvent,
    'deviceorientationevent': DeviceOrientationEvent,
    'gamepadevent': GamepadEvent,
    'mediaquerylistevent': MediaQueryListEvent,
    'formdataevent': FormDataEvent,
    'promiserejectionevent': PromiseRejectionEvent,
    'securitypolicyviolationevent': SecurityPolicyViolationEvent
  };

  document.createEvent = function(type) {
    var key = type.toLowerCase();
    var Ctor = aliases[key];
    if (!Ctor) {
      throw new DOMException("The operation is not supported.", "NotSupportedError");
    }
    var e = new Ctor('');
    e.initEvent = function(t, b, c) { this.type = t; this.bubbles = !!b; this.cancelable = !!c; };
    return e;
  };
})();

// ─── node.ownerDocument ─────────────────────────────────────────────────────
// WPT-tester kontrollerar ofta att noder hör till rätt dokument
(function() {
  if (typeof document === 'undefined') return;

  // Patcha createElement så att returnerade element har ownerDocument
  var _origCreateElement = document.createElement;
  if (_origCreateElement) {
    document.createElement = function(tag) {
      var el = _origCreateElement.call(document, tag);
      if (el && !('ownerDocument' in el)) {
        try {
          Object.defineProperty(el, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          el.ownerDocument = document;
        }
      }
      return el;
    };
  }

  // Patcha createTextNode
  var _origCreateTextNode = document.createTextNode;
  if (_origCreateTextNode) {
    document.createTextNode = function(text) {
      var node = _origCreateTextNode.call(document, text);
      if (node && !('ownerDocument' in node)) {
        try {
          Object.defineProperty(node, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          node.ownerDocument = document;
        }
      }
      return node;
    };
  }

  // Patcha createComment
  var _origCreateComment = document.createComment;
  if (_origCreateComment) {
    document.createComment = function(text) {
      var node = _origCreateComment.call(document, text);
      if (node && !('ownerDocument' in node)) {
        try {
          Object.defineProperty(node, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          node.ownerDocument = document;
        }
      }
      return node;
    };
  }

  // Patcha createDocumentFragment
  var _origCreateFragment = document.createDocumentFragment;
  if (_origCreateFragment) {
    document.createDocumentFragment = function() {
      var frag = _origCreateFragment.call(document);
      if (frag && !('ownerDocument' in frag)) {
        try {
          Object.defineProperty(frag, 'ownerDocument', {
            get: function() { return document; },
            configurable: true
          });
        } catch(e) {
          frag.ownerDocument = document;
        }
      }
      return frag;
    };
  }
})();

// ─── node.compareDocumentPosition() ─────────────────────────────────────────
// Returnerar bitmask: DISCONNECTED=1, PRECEDING=2, FOLLOWING=4,
// CONTAINS=8, CONTAINED_BY=16, IMPLEMENTATION_SPECIFIC=32
(function() {
  if (typeof Node === 'undefined') {
    if (typeof globalThis !== 'undefined') {
      globalThis.Node = {
        ELEMENT_NODE: 1,
        TEXT_NODE: 3,
        COMMENT_NODE: 8,
        DOCUMENT_NODE: 9,
        DOCUMENT_FRAGMENT_NODE: 11,
        DOCUMENT_POSITION_DISCONNECTED: 1,
        DOCUMENT_POSITION_PRECEDING: 2,
        DOCUMENT_POSITION_FOLLOWING: 4,
        DOCUMENT_POSITION_CONTAINS: 8,
        DOCUMENT_POSITION_CONTAINED_BY: 16,
        DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32
      };
    }
  }
})();

// ─── Element.remove() ───────────────────────────────────────────────────────
// Syntaktisk socker: el.remove() === el.parentNode.removeChild(el)
// Patcha via document.createElement wrapper
(function() {
  // Vi kan inte patcha alla element, men vi kan lägga till remove() på
  // element som getElementById/querySelector returnerar.
  // Bättre approach: patcha i en MutationObserver-liknande hook.
  // Enklaste: lägg till som polyfill-funktion som WPT-testerna kan använda.
  if (typeof document === 'undefined') return;

  // Konvertera argument till nod per spec: strings/null/undefined/numbers → textNode
  function toNode(arg) {
    if (arg && typeof arg === 'object' && (arg.nodeType || arg.appendChild)) return arg;
    return document.createTextNode(String(arg));
  }

  // Utility: lägg till ChildNode-metoder på ett element-objekt
  function patchChildNode(el) {
    if (!el || typeof el !== 'object') return el;
    // Sätt rätt prototypkedja (instanceof HTMLDivElement etc.)
    if (typeof __patchPrototype === 'function') __patchPrototype(el);
    // CharacterData-metoder för text/comment-noder
    if (typeof __patchCharacterData === 'function') __patchCharacterData(el);

    // remove(), before(), after() — nu Rust-native i dom_bridge.rs

    // after(), replaceWith() — nu Rust-native i dom_bridge.rs

    // prepend, append, replaceChildren — nu Rust-native i dom_bridge.rs

    return el;
  }

  // Gör patchChildNode tillgänglig globalt — runnern kan använda den
  globalThis.__patchChildNode = patchChildNode;

  // Patcha document.createElement
  var _ce = document.createElement;
  document.createElement = function(tag) {
    return patchChildNode(_ce.call(document, tag));
  };

  // Patcha document.getElementById
  var _gid = document.getElementById;
  if (_gid) {
    document.getElementById = function(id) {
      return patchChildNode(_gid.call(document, id));
    };
  }

  // Patcha document.querySelector
  var _qs = document.querySelector;
  if (_qs) {
    document.querySelector = function(sel) {
      return patchChildNode(_qs.call(document, sel));
    };
  }

  // Patcha document.querySelectorAll — returnerar array-like
  var _qsa = document.querySelectorAll;
  if (_qsa) {
    document.querySelectorAll = function(sel) {
      var result = _qsa.call(document, sel);
      if (result && result.length) {
        for (var i = 0; i < result.length; i++) {
          patchChildNode(result[i]);
        }
      }
      return result;
    };
  }

  // Patcha document.createTextNode
  var _ctn = document.createTextNode;
  document.createTextNode = function(text) {
    return patchChildNode(_ctn.call(document, text));
  };

  // Patcha document.createDocumentFragment
  var _cdf = document.createDocumentFragment;
  document.createDocumentFragment = function() {
    return patchChildNode(_cdf.call(document));
  };

  // Patcha document.createComment
  var _cc = document.createComment;
  document.createComment = function(text) {
    return patchChildNode(_cc.call(document, text));
  };
})();

// ─── Element.insertAdjacentElement() ────────────────────────────────────────
// insertAdjacentHTML finns redan — lägg till insertAdjacentElement
// och insertAdjacentText
(function() {
  // Dessa patchas via createElement-wrappern ovan,
  // men vi lägger också till dem i __patchChildNode
  var _origPatch = globalThis.__patchChildNode;
  globalThis.__patchChildNode = function(el) {
    if (!el || typeof el !== 'object') return el;
    el = _origPatch(el);

    // element.attributes — NamedNodeMap (live array-like av Attr-objekt)
    if (!el.attributes && el.nodeType === 1 && el.getAttributeNames) {
      Object.defineProperty(el, 'attributes', {
        get: function() {
          var self = this;
          var names = self.getAttributeNames ? self.getAttributeNames() : [];
          var map = [];
          var nsAttrs = self.__nsAttrs || {};
          // Samla NS-attribut
          var nsKeys = {};
          Object.keys(nsAttrs).forEach(function(key) {
            var a = nsAttrs[key];
            nsKeys[a.localName] = a;
          });
          for (var i = 0; i < names.length; i++) {
            var n = names[i];
            var v = self.getAttribute(n);
            var ns = nsKeys[n];
            map.push({
              name: ns ? ns.name : n,
              localName: ns ? ns.localName : n,
              value: ns ? ns.value : v,
              namespaceURI: ns ? ns.namespaceURI : null,
              prefix: ns ? ns.prefix : null,
              specified: true, ownerElement: self,
              nodeType: 2, nodeName: ns ? ns.name : n
            });
          }
          // Lägg till NS-attribut som inte finns i vanliga attribut
          Object.keys(nsAttrs).forEach(function(key) {
            var a = nsAttrs[key];
            if (names.indexOf(a.localName) === -1) {
              map.push({
                name: a.name, localName: a.localName, value: a.value,
                namespaceURI: a.namespaceURI, prefix: a.prefix,
                specified: true, ownerElement: self,
                nodeType: 2, nodeName: a.name
              });
            }
          });
          map.getNamedItem = function(name) {
            for (var j = 0; j < this.length; j++) {
              if (this[j].name === name) return this[j];
            }
            return null;
          };
          map.getNamedItemNS = function(ns, name) {
            for (var j = 0; j < this.length; j++) {
              if (this[j].localName === name && this[j].namespaceURI === ns) return this[j];
            }
            return null;
          };
          map.item = function(idx) { return this[idx] || null; };
          map.setNamedItem = function(attr) {
            self.setAttribute(attr.name, attr.value);
            return null;
          };
          map.removeNamedItem = function(name) {
            var old = self.getAttribute(name);
            self.removeAttribute(name);
            return { name: name, value: old, nodeType: 2, nodeName: name,
                     localName: name, namespaceURI: null, prefix: null };
          };
          return map;
        },
        configurable: true
      });
    }

    // getElementsByTagName, getElementsByClassName, getElementsByTagNameNS
    // — nu Rust-native i dom_bridge.rs

    // moveBefore — nu Rust-native i dom_bridge.rs

    // lookupNamespaceURI, lookupPrefix, isDefaultNamespace
    // — nu Rust-native i dom_bridge.rs

    // Namespace-metoder (NS-varianter)
    if (el.nodeType === 1) {
      // NS-metadata tracking — Rust lagrar värdet, JS spårar prefix/namespace
      if (el.setAttributeNS) {
        var _rustSetNS = el.setAttributeNS;
        el.__nsAttrs = el.__nsAttrs || {};
        el.setAttributeNS = function(ns, qname, val) {
          var parts = qname.split(':');
          var prefix = parts.length > 1 ? parts[0] : null;
          var local = parts.length > 1 ? parts[1] : qname;
          var key = (ns || '') + '|' + local;
          this.__nsAttrs = this.__nsAttrs || {};
          this.__nsAttrs[key] = { namespaceURI: ns, prefix: prefix, localName: local, value: String(val), name: qname };
          return _rustSetNS.call(this, ns, qname, val);
        };
      }
      if (!el.getAttributeNodeNS) {
        el.getAttributeNodeNS = function(ns, local) {
          if (!this.hasAttributeNS(ns, local)) return null;
          return {
            name: local, localName: local, value: this.getAttributeNS(ns, local),
            namespaceURI: ns, prefix: null, specified: true,
            ownerElement: this, nodeType: 2, nodeName: local
          };
        };
      }
    }

    // id, className — måste skriva tillbaka till arena via setAttribute
    if (el.nodeType === 1 && el.setAttribute) {
      var _origId = el.id || '';
      Object.defineProperty(el, 'id', {
        get: function() { return this.getAttribute('id') || ''; },
        set: function(v) { this.setAttribute('id', v); },
        configurable: true
      });
      var _origClass = el.className || '';
      Object.defineProperty(el, 'className', {
        get: function() { return this.getAttribute('class') || ''; },
        set: function(v) { this.setAttribute('class', v); },
        configurable: true
      });
    }

    // prefix, namespaceURI, localName — HTML-element har aldrig prefix/namespace
    if (el.nodeType === 1) {
      if (!('prefix' in el)) el.prefix = null;
      if (!('namespaceURI' in el)) el.namespaceURI = 'http://www.w3.org/1999/xhtml';
      if (!('localName' in el)) {
        el.localName = (el.tagName || '').toLowerCase();
      }
    }

    // toggleAttribute — nu Rust-native i dom_bridge.rs

    // getAttributeNode — nu Rust-native i dom_bridge.rs

    // getAttributeNames()
    if (!el.getAttributeNames && el.getAttribute) {
      // Kan inte implementera utan tillgång till attributlistan — hoppa
    }

    // insertAdjacentElement, insertAdjacentText — nu Rust-native i dom_bridge.rs

    return el;
  };
})();

// ─── DOMException ───────────────────────────────────────────────────────────
// WPT-tester kontrollerar DOMException-typ
(function() {
  if (typeof globalThis.DOMException === 'undefined') {
    globalThis.DOMException = function(message, name) {
      this.message = message || '';
      this.name = name || 'Error';
      this.code = DOMException._codes[this.name] || 0;
    };
    DOMException.prototype = Object.create(Error.prototype);
    DOMException.prototype.constructor = DOMException;
    DOMException._codes = {
      IndexSizeError: 1,
      HierarchyRequestError: 3,
      WrongDocumentError: 4,
      InvalidCharacterError: 5,
      NoModificationAllowedError: 7,
      NotFoundError: 8,
      NotSupportedError: 9,
      InvalidStateError: 11,
      SyntaxError: 12,
      InvalidModificationError: 13,
      NamespaceError: 14,
      SecurityError: 18,
      NetworkError: 19,
      AbortError: 20,
      TypeMismatchError: 17,
      QuotaExceededError: 22,
      DataCloneError: 25
    };
    // Statiska konstanter
    Object.keys(DOMException._codes).forEach(function(name) {
      DOMException[name.toUpperCase().replace(/ERROR$/, '_ERR')] = DOMException._codes[name];
    });
  }
})();

// ─── Synkronisera window med globalThis ─────────────────────────────────────
// WPT-tester använder "X in window" — allt på globalThis ska finnas på window
(function() {
  if (typeof window !== 'undefined' && typeof globalThis !== 'undefined') {
    // Gör window till en proxy för globalThis om möjligt
    // (window.HTMLAnchorElement etc. ska fungera)
    var origWindow = window;
    try {
      var handler = {
        get: function(target, prop) {
          if (prop in target) return target[prop];
          if (prop in globalThis) return globalThis[prop];
          return undefined;
        },
        has: function(target, prop) {
          return (prop in target) || (prop in globalThis);
        },
        set: function(target, prop, value) {
          target[prop] = value;
          return true;
        },
        deleteProperty: function(target, prop) {
          delete target[prop];
          delete globalThis[prop];
          return true;
        }
      };
      var proxyWin = new Proxy(origWindow, handler);
      globalThis.window = proxyWin;
      globalThis.self = proxyWin;
    } catch(e) {
      // Proxy inte tillgänglig — kopiera manuellt
    }
  }
})();

// ─── document.createElementNS ────────────────────────────────────────────────
(function() {
  if (typeof document === 'undefined') return;
  if (!document.createElementNS) {
    document.createElementNS = function(ns, qname) {
      var local = qname.indexOf(':') >= 0 ? qname.split(':')[1] : qname;
      var el = document.createElement(local);
      if (el) {
        try {
          Object.defineProperty(el, 'namespaceURI', { value: ns, configurable: true });
          if (qname.indexOf(':') >= 0) {
            Object.defineProperty(el, 'prefix', { value: qname.split(':')[0], configurable: true });
          }
          Object.defineProperty(el, 'localName', { value: local, configurable: true });
        } catch(e) {}
      }
      return el;
    };
  }
})();

// ─── document.getElementsByTagNameNS ─────────────────────────────────────────
(function() {
  if (typeof document === 'undefined') return;
  if (!document.getElementsByTagNameNS) {
    document.getElementsByTagNameNS = function(ns, tag) {
      if (tag === '*') return document.querySelectorAll('*');
      return document.querySelectorAll(tag.toLowerCase());
    };
  }
})();

// ─── NodeFilter konstanter ──────────────────────────────────────────────────
(function() {
  if (!globalThis.NodeFilter) globalThis.NodeFilter = {};
  NodeFilter.FILTER_ACCEPT = 1;
  NodeFilter.FILTER_REJECT = 2;
  NodeFilter.FILTER_SKIP = 3;
  NodeFilter.SHOW_ALL = 0xFFFFFFFF;
  NodeFilter.SHOW_ELEMENT = 0x1;
  NodeFilter.SHOW_ATTRIBUTE = 0x2;
  NodeFilter.SHOW_TEXT = 0x4;
  NodeFilter.SHOW_CDATA_SECTION = 0x8;
  NodeFilter.SHOW_PROCESSING_INSTRUCTION = 0x40;
  NodeFilter.SHOW_COMMENT = 0x80;
  NodeFilter.SHOW_DOCUMENT = 0x100;
  NodeFilter.SHOW_DOCUMENT_TYPE = 0x200;
  NodeFilter.SHOW_DOCUMENT_FRAGMENT = 0x400;
})();

// ─── Range implementation ───────────────────────────────────────────────────
// Riktig Range med setStart/setEnd, comparePoint, isPointInRange, intersectsNode
(function() {
  if (typeof document === 'undefined') return;

  function AetherRange() {
    this.startContainer = document;
    this.startOffset = 0;
    this.endContainer = document;
    this.endOffset = 0;
    this.collapsed = true;
    this.commonAncestorContainer = document;
  }

  AetherRange.prototype._update = function() {
    this.collapsed = (this.startContainer === this.endContainer && this.startOffset === this.endOffset);
    // Hitta gemensam ancestor
    var a = this.startContainer, b = this.endContainer;
    var ancestorsA = [];
    var node = a;
    while (node) { ancestorsA.push(node); node = node.parentNode; }
    node = b;
    while (node) {
      if (ancestorsA.indexOf(node) !== -1) { this.commonAncestorContainer = node; return; }
      node = node.parentNode;
    }
    this.commonAncestorContainer = document;
  };

  AetherRange.prototype.setStart = function(node, offset) {
    this.startContainer = node;
    this.startOffset = offset;
    // Om start > end, collapse till start
    if (this._compareBoundary(this.startContainer, this.startOffset, this.endContainer, this.endOffset) > 0) {
      this.endContainer = this.startContainer;
      this.endOffset = this.startOffset;
    }
    this._update();
  };

  AetherRange.prototype.setEnd = function(node, offset) {
    this.endContainer = node;
    this.endOffset = offset;
    if (this._compareBoundary(this.startContainer, this.startOffset, this.endContainer, this.endOffset) > 0) {
      this.startContainer = this.endContainer;
      this.startOffset = this.endOffset;
    }
    this._update();
  };

  AetherRange.prototype.setStartBefore = function(node) {
    var parent = node.parentNode;
    if (!parent) return;
    var idx = Array.from(parent.childNodes).indexOf(node);
    this.setStart(parent, idx);
  };

  AetherRange.prototype.setStartAfter = function(node) {
    var parent = node.parentNode;
    if (!parent) return;
    var idx = Array.from(parent.childNodes).indexOf(node);
    this.setStart(parent, idx + 1);
  };

  AetherRange.prototype.setEndBefore = function(node) {
    var parent = node.parentNode;
    if (!parent) return;
    var idx = Array.from(parent.childNodes).indexOf(node);
    this.setEnd(parent, idx);
  };

  AetherRange.prototype.setEndAfter = function(node) {
    var parent = node.parentNode;
    if (!parent) return;
    var idx = Array.from(parent.childNodes).indexOf(node);
    this.setEnd(parent, idx + 1);
  };

  AetherRange.prototype.collapse = function(toStart) {
    if (toStart) {
      this.endContainer = this.startContainer;
      this.endOffset = this.startOffset;
    } else {
      this.startContainer = this.endContainer;
      this.startOffset = this.endOffset;
    }
    this._update();
  };

  AetherRange.prototype.selectNode = function(node) {
    var parent = node.parentNode;
    if (!parent) return;
    var idx = Array.from(parent.childNodes).indexOf(node);
    this.setStart(parent, idx);
    this.setEnd(parent, idx + 1);
  };

  AetherRange.prototype.selectNodeContents = function(node) {
    this.startContainer = node;
    this.startOffset = 0;
    this.endContainer = node;
    this.endOffset = node.childNodes ? node.childNodes.length : (node.data ? node.data.length : 0);
    this._update();
  };

  AetherRange.prototype._compareBoundary = function(containerA, offsetA, containerB, offsetB) {
    // Same container → compare offsets
    if (containerA === containerB ||
        (containerA.__nodeKey__ && containerB.__nodeKey__ && containerA.__nodeKey__ === containerB.__nodeKey__)) {
      if (offsetA < offsetB) return -1;
      if (offsetA > offsetB) return 1;
      return 0;
    }
    if (!containerA.compareDocumentPosition) return 0;
    var pos = containerA.compareDocumentPosition(containerB);
    function indexOfChild(parent, child) {
      if (!parent.childNodes) return -1;
      for (var ci = 0; ci < parent.childNodes.length; ci++) {
        var ck = parent.childNodes[ci];
        if (ck === child || (ck.__nodeKey__ && child.__nodeKey__ && ck.__nodeKey__ === child.__nodeKey__)) return ci;
      }
      return -1;
    }
    if (pos & 16) {
      // B is CONTAINED_BY A — A ancestor of B
      var child = containerB;
      while (child.parentNode && child.parentNode !== containerA &&
             !(child.parentNode.__nodeKey__ && containerA.__nodeKey__ && child.parentNode.__nodeKey__ === containerA.__nodeKey__)) {
        child = child.parentNode;
      }
      if (child.parentNode) {
        var idx = indexOfChild(containerA, child);
        if (idx >= 0 && idx < offsetA) return 1;
        return -1;
      }
    }
    if (pos & 8) {
      // B CONTAINS A — B ancestor of A
      var child = containerA;
      while (child.parentNode && child.parentNode !== containerB &&
             !(child.parentNode.__nodeKey__ && containerB.__nodeKey__ && child.parentNode.__nodeKey__ === containerB.__nodeKey__)) {
        child = child.parentNode;
      }
      if (child.parentNode) {
        var idx = indexOfChild(containerB, child);
        if (idx >= 0 && idx < offsetB) return -1;
        return 1;
      }
    }
    if (pos & 4) return -1; // B follows A
    if (pos & 2) return 1;  // B precedes A
    return 0;
  };

  AetherRange.prototype.comparePoint = function(node, offset) {
    // Spec: root must match
    var nodeRoot = node;
    while (nodeRoot.parentNode) nodeRoot = nodeRoot.parentNode;
    var rangeRoot = this.startContainer;
    while (rangeRoot.parentNode) rangeRoot = rangeRoot.parentNode;
    if (nodeRoot !== rangeRoot && !(nodeRoot.__nodeKey__ && rangeRoot.__nodeKey__ && nodeRoot.__nodeKey__ === rangeRoot.__nodeKey__)) {
      throw new DOMException("Wrong document", "WrongDocumentError");
    }
    // Spec: offset must be valid
    var nodeLen = (node.nodeType === 3 || node.nodeType === 8 || node.nodeType === 7)
      ? (node.data !== undefined ? node.data.length : (node.textContent || "").length)
      : (node.childNodes ? node.childNodes.length : 0);
    if (offset < 0 || offset > nodeLen) {
      throw new DOMException("Index out of range", "IndexSizeError");
    }
    var cmpStart = this._compareBoundary(node, offset, this.startContainer, this.startOffset);
    if (cmpStart < 0) return -1;
    var cmpEnd = this._compareBoundary(node, offset, this.endContainer, this.endOffset);
    if (cmpEnd > 0) return 1;
    return 0;
  };

  AetherRange.prototype.isPointInRange = function(node, offset) {
    try { return this.comparePoint(node, offset) === 0; } catch(e) { return false; }
  };

  AetherRange.prototype.intersectsNode = function(node) {
    // Spec: om node och range har olika root → false
    var nodeRoot = node;
    while (nodeRoot.parentNode) nodeRoot = nodeRoot.parentNode;
    var rangeRoot = this.startContainer;
    while (rangeRoot.parentNode) rangeRoot = rangeRoot.parentNode;
    if (nodeRoot !== rangeRoot && !(nodeRoot.__nodeKey__ && rangeRoot.__nodeKey__ && nodeRoot.__nodeKey__ === rangeRoot.__nodeKey__)) return false;

    var parent = node.parentNode;
    if (!parent) return true;
    var kids = parent.childNodes;
    if (!kids) return true;
    var idx = -1;
    for (var i = 0; i < kids.length; i++) {
      if (kids[i] === node || (kids[i].__nodeKey__ && node.__nodeKey__ && kids[i].__nodeKey__ === node.__nodeKey__)) { idx = i; break; }
    }
    if (idx < 0) return true;
    var afterStart = this._compareBoundary(parent, idx + 1, this.startContainer, this.startOffset);
    var beforeEnd = this._compareBoundary(parent, idx, this.endContainer, this.endOffset);
    return afterStart > 0 && beforeEnd < 0;
  };

  AetherRange.prototype.cloneRange = function() {
    var r = new AetherRange();
    r.startContainer = this.startContainer;
    r.startOffset = this.startOffset;
    r.endContainer = this.endContainer;
    r.endOffset = this.endOffset;
    r._update();
    return r;
  };

  AetherRange.prototype.detach = function() {}; // no-op per spec

  AetherRange.prototype.toString = function() {
    // Returnera text inom range
    if (this.startContainer === this.endContainer && this.startContainer.nodeType === 3) {
      return (this.startContainer.data || '').substring(this.startOffset, this.endOffset);
    }
    return '';
  };

  AetherRange.prototype.deleteContents = function() {};
  AetherRange.prototype.extractContents = function() { return document.createDocumentFragment(); };
  AetherRange.prototype.cloneContents = function() { return document.createDocumentFragment(); };
  AetherRange.prototype.insertNode = function(node) {};
  AetherRange.prototype.surroundContents = function(node) {};

  AetherRange.prototype.getBoundingClientRect = function() {
    return { x: 0, y: 0, width: 0, height: 0, top: 0, right: 0, bottom: 0, left: 0 };
  };
  AetherRange.prototype.getClientRects = function() { return []; };

  AetherRange.START_TO_START = 0;
  AetherRange.START_TO_END = 1;
  AetherRange.END_TO_END = 2;
  AetherRange.END_TO_START = 3;

  AetherRange.prototype.compareBoundaryPoints = function(how, sourceRange) {
    var thisC, thisO, srcC, srcO;
    switch (how) {
      case 0: thisC = this.startContainer; thisO = this.startOffset; srcC = sourceRange.startContainer; srcO = sourceRange.startOffset; break;
      case 1: thisC = this.startContainer; thisO = this.startOffset; srcC = sourceRange.endContainer; srcO = sourceRange.endOffset; break;
      case 2: thisC = this.endContainer; thisO = this.endOffset; srcC = sourceRange.endContainer; srcO = sourceRange.endOffset; break;
      case 3: thisC = this.endContainer; thisO = this.endOffset; srcC = sourceRange.startContainer; srcO = sourceRange.startOffset; break;
      default: return 0;
    }
    return this._compareBoundary(thisC, thisO, srcC, srcO) < 0 ? -1 : (this._compareBoundary(thisC, thisO, srcC, srcO) > 0 ? 1 : 0);
  };

  // Override document.createRange
  document.createRange = function() { return new AetherRange(); };
  globalThis.Range = AetherRange;
})();

// ─── Document konstruktor → skapar riktig arena-backed doc ──────────────────
(function() {
  if (typeof document === 'undefined' || !document.implementation) return;
  var _origDocProto = globalThis.Document ? globalThis.Document.prototype : {};
  globalThis.Document = function Document() {
    // new Document() → createHTMLDocument utan titel
    var doc = document.implementation.createHTMLDocument('');
    // Sätt prototype för instanceof
    try { Object.setPrototypeOf(doc, _origDocProto); } catch(e) {}
    return doc;
  };
  globalThis.Document.prototype = _origDocProto;
  globalThis.Document.prototype.constructor = globalThis.Document;

  // XMLDocument — alias
  globalThis.XMLDocument = globalThis.Document;
})();

// ─── Patcha document.body/head/documentElement (pre-cache-skapade) ───────────
(function() {
  if (typeof document === 'undefined') return;
  // Dessa element skapades före polyfills — patcha dom nu
  [document.body, document.head, document.documentElement].forEach(function(el) {
    if (el && typeof __patchChildNode === 'function') __patchChildNode(el);
    if (el && typeof __patchPrototype === 'function') __patchPrototype(el);
  });
})();

// ─── NodeList.forEach ───────────────────────────────────────────────────────
// querySelectorAll returnerar array-like, men forEach behövs ofta
(function() {
  if (typeof NodeList === 'undefined') {
    globalThis.NodeList = function() {};
    NodeList.prototype = Object.create(Array.prototype);
  }
})();

// ─── DOM Type Hierarchy + instanceof-stöd ───────────────────────────────────
// Bygger en riktig prototypkedja: HTMLDivElement → HTMLElement → Element → Node → EventTarget
(function() {
  // Bas-konstruktorer med riktig prototypkedja
  function EventTarget() {}
  function NodeBase() {}
  NodeBase.prototype = Object.create(EventTarget.prototype);
  NodeBase.prototype.constructor = NodeBase;

  function ElementBase() {}
  ElementBase.prototype = Object.create(NodeBase.prototype);
  ElementBase.prototype.constructor = ElementBase;

  function CharacterDataBase() {}
  CharacterDataBase.prototype = Object.create(NodeBase.prototype);
  CharacterDataBase.prototype.constructor = CharacterDataBase;

  function HTMLElementBase() {}
  HTMLElementBase.prototype = Object.create(ElementBase.prototype);
  HTMLElementBase.prototype.constructor = HTMLElementBase;

  // Registrera bas-typer (om inte redan definierade)
  if (!globalThis.EventTarget) globalThis.EventTarget = EventTarget;
  if (!globalThis.Node) {
    globalThis.Node = NodeBase;
    // Node-konstanter
    Node.ELEMENT_NODE = 1; Node.ATTRIBUTE_NODE = 2; Node.TEXT_NODE = 3;
    Node.CDATA_SECTION_NODE = 4; Node.PROCESSING_INSTRUCTION_NODE = 7;
    Node.COMMENT_NODE = 8; Node.DOCUMENT_NODE = 9; Node.DOCUMENT_TYPE_NODE = 10;
    Node.DOCUMENT_FRAGMENT_NODE = 11;
    Node.DOCUMENT_POSITION_DISCONNECTED = 1; Node.DOCUMENT_POSITION_PRECEDING = 2;
    Node.DOCUMENT_POSITION_FOLLOWING = 4; Node.DOCUMENT_POSITION_CONTAINS = 8;
    Node.DOCUMENT_POSITION_CONTAINED_BY = 16; Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC = 32;
  }
  if (!globalThis.Element) globalThis.Element = ElementBase;
  if (!globalThis.CharacterData) globalThis.CharacterData = CharacterDataBase;
  if (!globalThis.HTMLElement) globalThis.HTMLElement = HTMLElementBase;

  // Tagnamn → konstruktor-mappning
  var tagMap = {};
  var htmlTypes = {
    'HTMLDivElement': ['div'],
    'HTMLSpanElement': ['span'],
    'HTMLParagraphElement': ['p'],
    'HTMLAnchorElement': ['a'],
    'HTMLButtonElement': ['button'],
    'HTMLInputElement': ['input'],
    'HTMLFormElement': ['form'],
    'HTMLSelectElement': ['select'],
    'HTMLOptionElement': ['option'],
    'HTMLTextAreaElement': ['textarea'],
    'HTMLImageElement': ['img'],
    'HTMLTableElement': ['table'],
    'HTMLTableRowElement': ['tr'],
    'HTMLTableCellElement': ['td', 'th'],
    'HTMLTableSectionElement': ['thead', 'tbody', 'tfoot'],
    'HTMLTableCaptionElement': ['caption'],
    'HTMLTableColElement': ['col', 'colgroup'],
    'HTMLHeadingElement': ['h1', 'h2', 'h3', 'h4', 'h5', 'h6'],
    'HTMLLabelElement': ['label'],
    'HTMLFieldSetElement': ['fieldset'],
    'HTMLLegendElement': ['legend'],
    'HTMLUListElement': ['ul'],
    'HTMLOListElement': ['ol'],
    'HTMLLIElement': ['li'],
    'HTMLDListElement': ['dl'],
    'HTMLPreElement': ['pre', 'listing', 'xmp'],
    'HTMLScriptElement': ['script'],
    'HTMLStyleElement': ['style'],
    'HTMLLinkElement': ['link'],
    'HTMLMetaElement': ['meta'],
    'HTMLBaseElement': ['base'],
    'HTMLBodyElement': ['body'],
    'HTMLHeadElement': ['head'],
    'HTMLHtmlElement': ['html'],
    'HTMLBRElement': ['br'],
    'HTMLHRElement': ['hr'],
    'HTMLIFrameElement': ['iframe'],
    'HTMLCanvasElement': ['canvas'],
    'HTMLVideoElement': ['video'],
    'HTMLAudioElement': ['audio'],
    'HTMLSourceElement': ['source'],
    'HTMLTemplateElement': ['template'],
    'HTMLSlotElement': ['slot'],
    'HTMLDataElement': ['data'],
    'HTMLTimeElement': ['time'],
    'HTMLOutputElement': ['output'],
    'HTMLProgressElement': ['progress'],
    'HTMLMeterElement': ['meter'],
    'HTMLDetailsElement': ['details'],
    'HTMLSummaryElement': ['summary'],
    'HTMLDialogElement': ['dialog'],
    'HTMLEmbedElement': ['embed'],
    'HTMLObjectElement': ['object'],
    'HTMLParamElement': ['param'],
    'HTMLTrackElement': ['track'],
    'HTMLAreaElement': ['area'],
    'HTMLMapElement': ['map'],
    'HTMLOptGroupElement': ['optgroup'],
    'HTMLDataListElement': ['datalist'],
    'HTMLModElement': ['ins', 'del'],
    'HTMLQuoteElement': ['blockquote', 'q'],
    'HTMLTitleElement': ['title'],
    'HTMLFontElement': ['font'],
    'HTMLDirectoryElement': ['dir'],
    'HTMLFrameElement': ['frame'],
    'HTMLFrameSetElement': ['frameset'],
    'HTMLMarqueeElement': ['marquee'],
    'HTMLMenuElement': ['menu'],
    'HTMLPictureElement': ['picture'],
    'HTMLUnknownElement': []
  };

  Object.keys(htmlTypes).forEach(function(name) {
    if (!globalThis[name]) {
      var Ctor = function() {};
      Ctor.prototype = Object.create(HTMLElementBase.prototype);
      Ctor.prototype.constructor = Ctor;
      globalThis[name] = Ctor;
    }
    htmlTypes[name].forEach(function(tag) {
      tagMap[tag.toUpperCase()] = globalThis[name];
    });
  });

  // Icke-HTML-typer
  var nonHtml = {
    'Text': CharacterDataBase,
    'Comment': CharacterDataBase,
    'DocumentFragment': NodeBase,
    'Document': NodeBase,
    'DocumentType': NodeBase,
    'ProcessingInstruction': CharacterDataBase,
    'CDATASection': CharacterDataBase,
    'Attr': NodeBase,
    'XMLDocument': NodeBase
  };
  Object.keys(nonHtml).forEach(function(name) {
    if (!globalThis[name]) {
      var Ctor = function() {};
      Ctor.prototype = Object.create(nonHtml[name].prototype);
      Ctor.prototype.constructor = Ctor;
      globalThis[name] = Ctor;
    }
  });

  // Utility-typer
  ['AbortSignal','DOMImplementation','NamedNodeMap','NodeList','HTMLCollection','DOMTokenList','DOMStringMap',
   'CSSStyleDeclaration','Range','Selection','TreeWalker','NodeIterator',
   'NodeFilter','MutationRecord','StaticRange','AbstractRange'
  ].forEach(function(name) {
    if (!globalThis[name]) {
      globalThis[name] = function() {};
      globalThis[name].prototype = {};
      globalThis[name].prototype.constructor = globalThis[name];
    }
  });

  // ─── setPrototypeOf på element-objekt ─────────────────────────────
  // make_element_object skapar plain objects — vi patchas via __patchProto
  globalThis.__tagToConstructor = tagMap;
  globalThis.__patchPrototype = function(el) {
    if (!el || typeof el !== 'object' || !el.tagName) return el;
    var nt = el.nodeType;
    var proto = null;
    if (nt === 1) {
      // Element — välj via tagName
      proto = tagMap[el.tagName] || globalThis.HTMLUnknownElement;
    } else if (nt === 3) {
      proto = globalThis.Text;
    } else if (nt === 8) {
      proto = globalThis.Comment;
    } else if (nt === 9) {
      proto = globalThis.Document;
    } else if (nt === 11) {
      proto = globalThis.DocumentFragment;
    } else if (nt === 10) {
      proto = globalThis.DocumentType;
    }
    if (proto && proto.prototype) {
      try { Object.setPrototypeOf(el, proto.prototype); } catch(e) {}
    }
    return el;
  };
})();
