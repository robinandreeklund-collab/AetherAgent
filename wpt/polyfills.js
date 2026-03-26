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
        // Delegera till native om möjlig
        if (document.createProcessingInstruction) {
          return document.createProcessingInstruction(target, data);
        }
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
      // Använd native arena-nod om tillgänglig (har ownerDocument getter)
      if (document.__createDocumentType) {
        return document.__createDocumentType(qualifiedName || '', publicId || '', systemId || '');
      }
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

// ─── Event-typ-konstruktorer (med spec-korrekta properties) ──────────────────
// Migrerad till dom_bridge.rs-stil: ärver Event, lägger till subclass-properties.
(function() {
  if (typeof Event === 'undefined') return;

  // UIEvent — bas för Mouse/Keyboard/Focus/Input
  if (!globalThis.UIEvent) {
    globalThis.UIEvent = function UIEvent(type, opts) {
      Event.call(this, type, opts);
      this.view = (opts && opts.view) || null;
      this.detail = (opts && opts.detail !== undefined) ? opts.detail : 0;
    };
    UIEvent.prototype = Object.create(Event.prototype);
    UIEvent.prototype.constructor = UIEvent;
    UIEvent.prototype.initUIEvent = function(t, b, c, v, d) { this.initEvent(t, b, c); this.view = v || null; this.detail = d || 0; };
  }

  // MouseEvent
  if (!globalThis.MouseEvent) {
    globalThis.MouseEvent = function MouseEvent(type, opts) {
      UIEvent.call(this, type, opts);
      var o = opts || {};
      this.screenX = o.screenX || 0; this.screenY = o.screenY || 0;
      this.clientX = o.clientX || 0; this.clientY = o.clientY || 0;
      this.pageX = o.pageX || 0; this.pageY = o.pageY || 0;
      this.offsetX = o.offsetX || 0; this.offsetY = o.offsetY || 0;
      this.movementX = o.movementX || 0; this.movementY = o.movementY || 0;
      this.button = o.button || 0; this.buttons = o.buttons || 0;
      this.relatedTarget = o.relatedTarget || null;
      this.ctrlKey = !!o.ctrlKey; this.shiftKey = !!o.shiftKey;
      this.altKey = !!o.altKey; this.metaKey = !!o.metaKey;
    };
    MouseEvent.prototype = Object.create(UIEvent.prototype);
    MouseEvent.prototype.constructor = MouseEvent;
    MouseEvent.prototype.initMouseEvent = function(t,b,c,v,d,sx,sy,cx,cy,ctrl,alt,shift,meta,btn,rt) {
      this.initUIEvent(t,b,c,v,d); this.screenX=sx||0; this.screenY=sy||0; this.clientX=cx||0; this.clientY=cy||0;
      this.ctrlKey=!!ctrl; this.altKey=!!alt; this.shiftKey=!!shift; this.metaKey=!!meta; this.button=btn||0; this.relatedTarget=rt||null;
    };
    MouseEvent.prototype.getModifierState = function(key) {
      if (key === 'Control') return this.ctrlKey; if (key === 'Shift') return this.shiftKey;
      if (key === 'Alt') return this.altKey; if (key === 'Meta') return this.metaKey; return false;
    };
  }

  // KeyboardEvent
  if (!globalThis.KeyboardEvent) {
    globalThis.KeyboardEvent = function KeyboardEvent(type, opts) {
      UIEvent.call(this, type, opts);
      var o = opts || {};
      this.key = o.key || ''; this.code = o.code || '';
      this.location = o.location || 0;
      this.repeat = !!o.repeat; this.isComposing = !!o.isComposing;
      this.ctrlKey = !!o.ctrlKey; this.shiftKey = !!o.shiftKey;
      this.altKey = !!o.altKey; this.metaKey = !!o.metaKey;
      this.charCode = o.charCode || 0; this.keyCode = o.keyCode || 0; this.which = o.which || 0;
    };
    KeyboardEvent.prototype = Object.create(UIEvent.prototype);
    KeyboardEvent.prototype.constructor = KeyboardEvent;
    KeyboardEvent.prototype.getModifierState = function(key) {
      if (key === 'Control') return this.ctrlKey; if (key === 'Shift') return this.shiftKey;
      if (key === 'Alt') return this.altKey; if (key === 'Meta') return this.metaKey; return false;
    };
    KeyboardEvent.DOM_KEY_LOCATION_STANDARD = 0; KeyboardEvent.DOM_KEY_LOCATION_LEFT = 1;
    KeyboardEvent.DOM_KEY_LOCATION_RIGHT = 2; KeyboardEvent.DOM_KEY_LOCATION_NUMPAD = 3;
  }

  // FocusEvent
  if (!globalThis.FocusEvent) {
    globalThis.FocusEvent = function FocusEvent(type, opts) {
      UIEvent.call(this, type, opts);
      this.relatedTarget = (opts && opts.relatedTarget) || null;
    };
    FocusEvent.prototype = Object.create(UIEvent.prototype);
    FocusEvent.prototype.constructor = FocusEvent;
  }

  // InputEvent
  if (!globalThis.InputEvent) {
    globalThis.InputEvent = function InputEvent(type, opts) {
      UIEvent.call(this, type, opts);
      var o = opts || {};
      this.data = o.data !== undefined ? o.data : null;
      this.inputType = o.inputType || '';
      this.isComposing = !!o.isComposing;
      this.dataTransfer = o.dataTransfer || null;
    };
    InputEvent.prototype = Object.create(UIEvent.prototype);
    InputEvent.prototype.constructor = InputEvent;
  }

  // WheelEvent
  if (!globalThis.WheelEvent) {
    globalThis.WheelEvent = function WheelEvent(type, opts) {
      MouseEvent.call(this, type, opts);
      var o = opts || {};
      this.deltaX = o.deltaX || 0; this.deltaY = o.deltaY || 0; this.deltaZ = o.deltaZ || 0;
      this.deltaMode = o.deltaMode || 0;
    };
    WheelEvent.prototype = Object.create(MouseEvent.prototype);
    WheelEvent.prototype.constructor = WheelEvent;
    WheelEvent.DOM_DELTA_PIXEL = 0; WheelEvent.DOM_DELTA_LINE = 1; WheelEvent.DOM_DELTA_PAGE = 2;
  }

  // PointerEvent
  if (!globalThis.PointerEvent) {
    globalThis.PointerEvent = function PointerEvent(type, opts) {
      MouseEvent.call(this, type, opts);
      var o = opts || {};
      this.pointerId = o.pointerId || 0; this.width = o.width || 1; this.height = o.height || 1;
      this.pressure = o.pressure || 0; this.tangentialPressure = o.tangentialPressure || 0;
      this.tiltX = o.tiltX || 0; this.tiltY = o.tiltY || 0; this.twist = o.twist || 0;
      this.pointerType = o.pointerType || ''; this.isPrimary = !!o.isPrimary;
    };
    PointerEvent.prototype = Object.create(MouseEvent.prototype);
    PointerEvent.prototype.constructor = PointerEvent;
  }

  // Enklare event-typer (ärver Event direkt)
  // CompositionEvent — har data property
  if (!globalThis.CompositionEvent) {
    globalThis.CompositionEvent = function CompositionEvent(type, opts) {
      UIEvent.call(this, type, opts);
      this.data = (opts && opts.data !== undefined) ? opts.data : '';
    };
    CompositionEvent.prototype = Object.create(UIEvent.prototype);
    CompositionEvent.prototype.constructor = CompositionEvent;
  }

  var simpleTypes = [
    'TouchEvent', 'AnimationEvent', 'TransitionEvent',
    'HashChangeEvent', 'PopStateEvent', 'StorageEvent', 'PageTransitionEvent',
    'ProgressEvent', 'ClipboardEvent', 'DragEvent', 'ErrorEvent',
    'MessageEvent', 'PromiseRejectionEvent', 'SecurityPolicyViolationEvent',
    'DeviceMotionEvent', 'DeviceOrientationEvent', 'GamepadEvent',
    'MediaQueryListEvent', 'FormDataEvent', 'SubmitEvent', 'BeforeUnloadEvent'
  ];
  simpleTypes.forEach(function(name) {
    if (!globalThis[name]) {
      globalThis[name] = function(type, opts) { Event.call(this, type, opts); };
      globalThis[name].prototype = Object.create(Event.prototype);
      globalThis[name].prototype.constructor = globalThis[name];
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
// MIGRERAD TILL RUST (2026-03-25) — registreras native via register_dom_exception()
// Polyfill borttagen. DOMException skapas nu i dom_bridge.rs innan polyfills laddas.

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

// Range API — nu native i dom_bridge.rs (migrerad 2026-03-25)
// document.createAttribute (behövs fortfarande som polyfill)
(function() {
  if (typeof document === 'undefined') return;
  if (!document.createAttribute) {
    document.createAttribute = function(name) {
      var attr = { nodeType: 2, nodeName: name.toLowerCase(), name: name.toLowerCase(), value: '', nodeValue: '', specified: true,
        ownerElement: null, ownerDocument: document,
        toString: function() { return '[object Attr]'; }
      };
      Object.defineProperty(attr, Symbol.toStringTag, { value: 'Attr' });
      return attr;
    };
  }
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
