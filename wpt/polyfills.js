/*
 * AetherAgent WPT Polyfills
 *
 * Fyller luckor i DOM bridge som WPT-tester förväntar sig.
 * Laddas före testharness.js.
 */

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

  // Utility: lägg till ChildNode-metoder på ett element-objekt
  function patchChildNode(el) {
    if (!el || typeof el !== 'object') return el;

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
          var node = arguments[i];
          if (typeof node === 'string') {
            node = document.createTextNode(node);
          }
          parent.insertBefore(node, this);
        }
      };
    }

    if (!el.after) {
      el.after = function() {
        var parent = this.parentNode;
        if (!parent) return;
        var ref = this.nextSibling;
        for (var i = 0; i < arguments.length; i++) {
          var node = arguments[i];
          if (typeof node === 'string') {
            node = document.createTextNode(node);
          }
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
          var node = arguments[i];
          if (typeof node === 'string') {
            node = document.createTextNode(node);
          }
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
          var node = arguments[i];
          if (typeof node === 'string') {
            node = document.createTextNode(node);
          }
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
          var node = arguments[i];
          if (typeof node === 'string') {
            node = document.createTextNode(node);
          }
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
          var node = arguments[i];
          if (typeof node === 'string') {
            node = document.createTextNode(node);
          }
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

// ─── NodeList.forEach ───────────────────────────────────────────────────────
// querySelectorAll returnerar array-like, men forEach behövs ofta
(function() {
  if (typeof NodeList === 'undefined') {
    globalThis.NodeList = function() {};
    NodeList.prototype = Object.create(Array.prototype);
  }
})();

// ─── HTMLElement typer ──────────────────────────────────────────────────────
// WPT-tester kontrollerar ofta instanceof HTMLDivElement etc.
(function() {
  var types = [
    'HTMLElement', 'HTMLDivElement', 'HTMLSpanElement', 'HTMLParagraphElement',
    'HTMLAnchorElement', 'HTMLButtonElement', 'HTMLInputElement',
    'HTMLFormElement', 'HTMLSelectElement', 'HTMLOptionElement',
    'HTMLTextAreaElement', 'HTMLImageElement', 'HTMLTableElement',
    'HTMLTableRowElement', 'HTMLTableCellElement', 'HTMLListElement',
    'HTMLHeadingElement', 'HTMLLabelElement', 'HTMLFieldSetElement',
    'HTMLLegendElement', 'HTMLUListElement', 'HTMLOListElement',
    'HTMLLIElement', 'HTMLPreElement', 'HTMLScriptElement',
    'HTMLStyleElement', 'HTMLLinkElement', 'HTMLMetaElement',
    'HTMLBodyElement', 'HTMLHeadElement', 'HTMLHtmlElement',
    'HTMLBRElement', 'HTMLHRElement', 'HTMLIFrameElement',
    'HTMLCanvasElement', 'HTMLVideoElement', 'HTMLAudioElement',
    'HTMLSourceElement', 'HTMLTemplateElement', 'HTMLSlotElement',
    'HTMLUnknownElement', 'HTMLDataElement', 'HTMLTimeElement',
    'HTMLOutputElement', 'HTMLProgressElement', 'HTMLMeterElement',
    'HTMLDetailsElement', 'HTMLSummaryElement', 'HTMLDialogElement',
    'Text', 'Comment', 'DocumentFragment', 'Document',
    'Element', 'CharacterData', 'Attr', 'NamedNodeMap',
    'NodeList', 'HTMLCollection', 'DOMTokenList', 'DOMStringMap',
    'CSSStyleDeclaration', 'Range', 'Selection',
    'TreeWalker', 'NodeIterator', 'NodeFilter',
    'MutationRecord', 'StaticRange', 'AbstractRange',
    'XMLDocument', 'DocumentType', 'ProcessingInstruction',
    'CDATASection'
  ];

  types.forEach(function(name) {
    if (typeof globalThis[name] === 'undefined') {
      globalThis[name] = function() {};
      globalThis[name].prototype = {};
      globalThis[name].prototype.constructor = globalThis[name];
    }
  });
})();
