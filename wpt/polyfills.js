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

    // .data getter/setter — alias för textContent
    if (!('data' in node)) {
      Object.defineProperty(node, 'data', {
        get: function() { return this.textContent || ''; },
        set: function(val) { this.textContent = String(val); },
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

    // .substringData(offset, count)
    if (!node.substringData) {
      node.substringData = function(offset, count) {
        if (arguments.length < 2) throw new TypeError("Not enough arguments");
        var d = this.data;
        offset = offset >>> 0; // ToUint32 then back
        if (offset > d.length) {
          throw new DOMException("Offset is out of range", "IndexSizeError");
        }
        count = count >>> 0;
        return d.substring(offset, offset + count);
      };
    }

    // .appendData(data)
    if (!node.appendData) {
      node.appendData = function(data) {
        this.data += String(data);
      };
    }

    // .insertData(offset, data)
    if (!node.insertData) {
      node.insertData = function(offset, data) {
        var d = this.data;
        offset = offset >>> 0;
        if (offset > d.length) {
          throw new DOMException("Offset is out of range", "IndexSizeError");
        }
        this.data = d.substring(0, offset) + String(data) + d.substring(offset);
      };
    }

    // .deleteData(offset, count)
    if (!node.deleteData) {
      node.deleteData = function(offset, count) {
        var d = this.data;
        offset = offset >>> 0;
        if (offset > d.length) {
          throw new DOMException("Offset is out of range", "IndexSizeError");
        }
        count = count >>> 0;
        var end = Math.min(offset + count, d.length);
        this.data = d.substring(0, offset) + d.substring(end);
      };
    }

    // .replaceData(offset, count, data)
    if (!node.replaceData) {
      node.replaceData = function(offset, count, data) {
        var d = this.data;
        offset = offset >>> 0;
        if (offset > d.length) {
          throw new DOMException("Offset is out of range", "IndexSizeError");
        }
        count = count >>> 0;
        var end = Math.min(offset + count, d.length);
        this.data = d.substring(0, offset) + String(data) + d.substring(end);
      };
    }

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
      // Returnera ett minimalt dokument-liknande objekt
      var doc = {
        nodeType: 9,
        nodeName: '#document',
        childNodes: [],
        children: [],
        documentElement: null,
        head: null,
        body: null,
        title: title || '',
        implementation: document.implementation,
        createElement: document.createElement.bind(document),
        createTextNode: document.createTextNode.bind(document),
        createComment: document.createComment.bind(document),
        createDocumentFragment: document.createDocumentFragment.bind(document),
        getElementById: function() { return null; },
        querySelector: function() { return null; },
        querySelectorAll: function() { return []; },
        getElementsByClassName: function() { return []; },
        getElementsByTagName: function() { return []; },
        addEventListener: function() {},
        removeEventListener: function() {},
        dispatchEvent: function() { return true; }
      };
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

// ─── document.createEvent() ─────────────────────────────────────────────────
// testharness.js och många WPT-tester använder detta
(function() {
  if (typeof document !== 'undefined' && !document.createEvent) {
    document.createEvent = function(type) {
      var e = {
        type: '',
        bubbles: false,
        cancelable: false,
        defaultPrevented: false,
        target: null,
        currentTarget: null,
        eventPhase: 0,
        timeStamp: Date.now(),
        isTrusted: false,
        preventDefault: function() { this.defaultPrevented = true; },
        stopPropagation: function() {},
        stopImmediatePropagation: function() {},
        initEvent: function(t, b, c) {
          this.type = t;
          this.bubbles = !!b;
          this.cancelable = !!c;
        }
      };
      return e;
    };
  }
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

    if (!el.remove) {
      el.remove = function() {
        if (this.parentNode) {
          this.parentNode.removeChild(this);
        }
      };
    }

    if (!el.before) {
      el.before = function() {
        var parent = this.parentNode;
        if (!parent) return;
        for (var i = 0; i < arguments.length; i++) {
          parent.insertBefore(toNode(arguments[i]), this);
        }
      };
    }

    if (!el.after) {
      el.after = function() {
        var parent = this.parentNode;
        if (!parent) return;
        var ref = this.nextSibling;
        for (var i = 0; i < arguments.length; i++) {
          var node = toNode(arguments[i]);
          if (ref) {
            parent.insertBefore(node, ref);
          } else {
            parent.appendChild(node);
          }
        }
      };
    }

    if (!el.replaceWith) {
      el.replaceWith = function() {
        var parent = this.parentNode;
        if (!parent) return;
        var ref = this.nextSibling;
        parent.removeChild(this);
        for (var i = 0; i < arguments.length; i++) {
          var node = toNode(arguments[i]);
          if (ref) {
            parent.insertBefore(node, ref);
          } else {
            parent.appendChild(node);
          }
        }
      };
    }

    if (!el.prepend) {
      el.prepend = function() {
        var ref = this.firstChild;
        for (var i = 0; i < arguments.length; i++) {
          var node = toNode(arguments[i]);
          if (ref) {
            this.insertBefore(node, ref);
          } else {
            this.appendChild(node);
          }
        }
      };
    }

    if (!el.append) {
      el.append = function() {
        for (var i = 0; i < arguments.length; i++) {
          var node = toNode(arguments[i]);
          this.appendChild(node);
        }
      };
    }

    if (!el.replaceChildren) {
      el.replaceChildren = function() {
        while (this.firstChild) {
          this.removeChild(this.firstChild);
        }
        for (var i = 0; i < arguments.length; i++) {
          var node = toNode(arguments[i]);
          this.appendChild(node);
        }
      };
    }

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
          for (var i = 0; i < names.length; i++) {
            var n = names[i];
            var v = self.getAttribute(n);
            map.push({
              name: n, localName: n, value: v,
              namespaceURI: null, prefix: null,
              specified: true, ownerElement: self,
              nodeType: 2, nodeName: n
            });
          }
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

    // toggleAttribute(name [, force])
    if (!el.toggleAttribute) {
      el.toggleAttribute = function(name, force) {
        if (arguments.length > 1) {
          if (force) { this.setAttribute(name, ''); return true; }
          else { this.removeAttribute(name); return false; }
        }
        if (this.hasAttribute(name)) { this.removeAttribute(name); return false; }
        else { this.setAttribute(name, ''); return true; }
      };
    }

    // getAttributeNode(name) — returnerar Attr-liknande objekt
    if (!el.getAttributeNode) {
      el.getAttributeNode = function(name) {
        if (!this.hasAttribute(name)) return null;
        var val = this.getAttribute(name);
        return {
          name: name.toLowerCase(),
          localName: name.toLowerCase(),
          value: val,
          namespaceURI: null,
          prefix: null,
          specified: true,
          ownerElement: this,
          nodeType: 2,
          nodeName: name.toLowerCase()
        };
      };
    }

    // getAttributeNames()
    if (!el.getAttributeNames && el.getAttribute) {
      // Kan inte implementera utan tillgång till attributlistan — hoppa
    }

    if (!el.insertAdjacentElement) {
      el.insertAdjacentElement = function(position, element) {
        switch (position.toLowerCase()) {
          case 'beforebegin':
            if (this.parentNode) this.parentNode.insertBefore(element, this);
            break;
          case 'afterbegin':
            this.insertBefore(element, this.firstChild);
            break;
          case 'beforeend':
            this.appendChild(element);
            break;
          case 'afterend':
            if (this.parentNode) {
              if (this.nextSibling) {
                this.parentNode.insertBefore(element, this.nextSibling);
              } else {
                this.parentNode.appendChild(element);
              }
            }
            break;
        }
        return element;
      };
    }

    if (!el.insertAdjacentText) {
      el.insertAdjacentText = function(position, text) {
        var node = document.createTextNode(text);
        this.insertAdjacentElement(position, node);
      };
    }

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
  ['NamedNodeMap','NodeList','HTMLCollection','DOMTokenList','DOMStringMap',
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
