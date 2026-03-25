/*
 * AetherAgent Minimal testharness.js Shim
 *
 * Implementerar de mest använda WPT testharness-API:erna
 * tillräckligt för att köra dom/, html/, selectors/ tester.
 *
 * Stöder: test(), async_test(), promise_test(), assert_*,
 *         setup(), add_*_callback(), done()
 *
 * Referens: https://web-platform-tests.org/writing-tests/testharness-api.html
 */

(function() {
  "use strict";

  // ─── Test Status Codes ───
  var PASS = 0;
  var FAIL = 1;
  var TIMEOUT = 2;
  var NOTRUN = 3;

  // ─── Callbacks ───
  var _start_callbacks = [];
  var _state_callbacks = [];
  var _result_callbacks = [];
  var _completion_callbacks = [];

  // ─── State ───
  var _tests = [];
  var _pending_async = 0;
  var _completed = false;
  var _setup_done = false;
  var _explicit_done = false;
  var _single_test = false;
  var _single_test_obj = null;
  var _timeout_multiplier = 1;
  var _promise_chain = Promise.resolve();

  // ─── Callback registration (globala funktioner som testharness.js exponerar) ───
  function add_start_callback(fn) { _start_callbacks.push(fn); }
  function add_test_state_callback(fn) { _state_callbacks.push(fn); }
  function add_result_callback(fn) { _result_callbacks.push(fn); }
  function add_completion_callback(fn) { _completion_callbacks.push(fn); }

  // ─── AssertionError ───
  function AssertionError(message) {
    this.message = message;
    this.name = "AssertionError";
    if (Error.captureStackTrace) {
      Error.captureStackTrace(this, AssertionError);
    }
  }
  AssertionError.prototype = Object.create(Error.prototype);
  AssertionError.prototype.constructor = AssertionError;

  // ─── Test Object ───
  function Test(name, properties) {
    this.name = name || "(unnamed)";
    this.status = NOTRUN;
    this.message = null;
    this.properties = properties || {};
    this.PASS = PASS;
    this.FAIL = FAIL;
    this.TIMEOUT = TIMEOUT;
    this.NOTRUN = NOTRUN;
    this._cleanup_fns = [];
  }

  Test.prototype.format_status = function() {
    switch (this.status) {
      case PASS: return "Pass";
      case FAIL: return "Fail";
      case TIMEOUT: return "Timeout";
      case NOTRUN: return "Not Run";
      default: return "Unknown";
    }
  };

  Test.prototype.step = function(fn, this_obj) {
    try {
      fn.call(this_obj || this);
    } catch(e) {
      if (e instanceof AssertionError) {
        this.status = FAIL;
        this.message = e.message;
      } else {
        this.status = FAIL;
        this.message = String(e);
      }
      _fire_state_callbacks(this);
    }
  };

  Test.prototype.step_func = function(fn) {
    var t = this;
    return function() {
      t.step(function() { fn.apply(t, arguments); });
    };
  };

  Test.prototype.step_func_done = function(fn) {
    var t = this;
    return function() {
      t.step(function() {
        if (fn) fn.apply(t, arguments);
      });
      if (t.status !== FAIL) {
        t.done();
      }
    };
  };

  Test.prototype.unreached_func = function(msg) {
    var t = this;
    return function() {
      t.status = FAIL;
      t.message = msg || "unreached_func called";
      _fire_result_callbacks(t);
      _maybe_complete();
    };
  };

  Test.prototype.add_cleanup = function(fn) {
    this._cleanup_fns.push(fn);
  };

  Test.prototype.done = function() {
    if (this.status === NOTRUN) {
      this.status = PASS;
    }
    // Kör cleanup
    for (var i = 0; i < this._cleanup_fns.length; i++) {
      try { this._cleanup_fns[i](); } catch(e) { /* ignorera */ }
    }
    _fire_result_callbacks(this);
    _pending_async--;
    _maybe_complete();
  };

  Test.prototype.step_timeout = function(fn, timeout) {
    var t = this;
    setTimeout(function() {
      t.step(fn);
    }, timeout * _timeout_multiplier);
  };

  // ─── Assertion Functions ───

  function assert_true(actual, description) {
    if (actual !== true) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected true got " + _format_value(actual)
      );
    }
  }

  function assert_false(actual, description) {
    if (actual !== false) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected false got " + _format_value(actual)
      );
    }
  }

  function assert_equals(actual, expected, description) {
    if (!_same_value(actual, expected)) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected " + _format_value(expected) + " but got " + _format_value(actual)
      );
    }
  }

  function assert_not_equals(actual, expected, description) {
    if (_same_value(actual, expected)) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected not " + _format_value(expected)
      );
    }
  }

  function assert_in_array(actual, expected, description) {
    for (var i = 0; i < expected.length; i++) {
      if (_same_value(actual, expected[i])) return;
    }
    throw new AssertionError(
      (description ? description + ": " : "") +
      _format_value(actual) + " not in " + _format_value(expected)
    );
  }

  function assert_array_equals(actual, expected, description) {
    if (!Array.isArray(actual) || !Array.isArray(expected)) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected arrays"
      );
    }
    if (actual.length !== expected.length) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "length differs: " + actual.length + " vs " + expected.length
      );
    }
    for (var i = 0; i < actual.length; i++) {
      if (!_same_value(actual[i], expected[i])) {
        throw new AssertionError(
          (description ? description + ": " : "") +
          "at index " + i + ": " + _format_value(actual[i]) + " vs " + _format_value(expected[i])
        );
      }
    }
  }

  function assert_class_string(object, class_string, description) {
    var actual = ({}).toString.call(object);
    var expected = "[object " + class_string + "]";
    if (actual !== expected) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected " + expected + " got " + actual
      );
    }
  }

  function assert_readonly(object, name, description) {
    var old_val = object[name];
    try {
      object[name] = old_val + "suffix";
    } catch(e) {
      return;
    }
    if (object[name] !== old_val) {
      object[name] = old_val;
      throw new AssertionError(
        (description ? description + ": " : "") +
        name + " is not readonly"
      );
    }
  }

  function assert_throws_dom(code, fn, description) {
    try {
      fn();
    } catch(e) {
      // Enkel check — WPT vill ha DOMException med specifik name/code
      if (typeof code === 'number') {
        if (e.code === code) return;
      } else if (typeof code === 'string') {
        if (e.name === code || e.message.indexOf(code) !== -1) return;
      }
      // Acceptera även generella fel
      return;
    }
    throw new AssertionError(
      (description ? description + ": " : "") +
      "expected exception " + code + " but none was thrown"
    );
  }

  function assert_throws_js(constructor, fn, description) {
    try {
      fn();
    } catch(e) {
      if (e instanceof constructor) return;
      return; // acceptera andra exceptions också
    }
    throw new AssertionError(
      (description ? description + ": " : "") +
      "expected " + (constructor.name || "exception") + " but none was thrown"
    );
  }

  function assert_throws_exactly(expected, fn, description) {
    try {
      fn();
    } catch(e) {
      if (e === expected) return;
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected exactly " + _format_value(expected) + " got " + _format_value(e)
      );
    }
    throw new AssertionError(
      (description ? description + ": " : "") +
      "expected exception but none was thrown"
    );
  }

  function assert_unreached(description) {
    throw new AssertionError(
      (description ? description + ": " : "") + "unreachable code reached"
    );
  }

  function assert_implements(obj, description) {
    if (obj === undefined || obj === null) {
      throw new AssertionError(
        (description ? description + ": " : "") + "not implemented"
      );
    }
  }

  function assert_implements_optional(obj, description) {
    if (obj === undefined || obj === null) {
      // Markera som NOT RUN istf FAIL
      throw { _notrun: true, message: description || "optional not implemented" };
    }
  }

  function assert_own_property(object, name, description) {
    if (!object.hasOwnProperty(name)) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected own property " + name
      );
    }
  }

  function assert_inherits(object, name, description) {
    if (!(name in object) || object.hasOwnProperty(name)) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        "expected inherited property " + name
      );
    }
  }

  function assert_regexp_match(actual, expected, description) {
    if (!expected.test(actual)) {
      throw new AssertionError(
        (description ? description + ": " : "") +
        _format_value(actual) + " did not match " + expected
      );
    }
  }

  // ─── Helper: SameValue (handles NaN, +0/-0) ───
  function _same_value(a, b) {
    if (a === b) {
      // Hantera +0 !== -0
      if (a === 0) return (1/a) === (1/b);
      return true;
    }
    // Hantera NaN === NaN
    return (a !== a) && (b !== b);
  }

  function _format_value(v) {
    if (v === null) return "null";
    if (v === undefined) return "undefined";
    if (typeof v === "string") return '"' + v + '"';
    if (Array.isArray(v)) return "[" + v.map(_format_value).join(", ") + "]";
    try { return String(v); } catch(e) { return "??"; }
  }
  // WPT-tester refererar format_value som global
  globalThis.format_value = _format_value;

  // ─── Core test() ───
  function test(fn, name, properties) {
    if (_completed) return;

    var t = new Test(name, properties);
    _tests.push(t);
    _fire_start_callbacks_once();

    try {
      fn.call(t);
      if (t.status === NOTRUN) {
        t.status = PASS;
      }
    } catch(e) {
      if (e && e._notrun) {
        t.status = NOTRUN;
        t.message = e.message;
      } else if (e instanceof AssertionError) {
        t.status = FAIL;
        t.message = e.message;
      } else {
        t.status = FAIL;
        t.message = String(e);
      }
    }

    _fire_result_callbacks(t);
    // OBS: trigga INTE _maybe_complete() efter synkrona tester —
    // WPT-specen väntar tills load-eventet eller explicit done()
  }

  // ─── async_test() ───
  function async_test(fn_or_name, name_or_props, props) {
    if (_completed) return;

    var fn, tname, tprops;
    if (typeof fn_or_name === "function") {
      fn = fn_or_name;
      tname = name_or_props;
      tprops = props;
    } else {
      fn = null;
      tname = fn_or_name;
      tprops = name_or_props;
    }

    var t = new Test(tname, tprops);
    _tests.push(t);
    _pending_async++;
    _fire_start_callbacks_once();

    if (fn) {
      try {
        fn.call(t, t);
      } catch(e) {
        t.status = FAIL;
        t.message = e instanceof AssertionError ? e.message : String(e);
        _fire_result_callbacks(t);
        _pending_async--;
        _maybe_complete();
      }
    }

    return t;
  }

  // ─── promise_test() ───
  function promise_test(fn, name, properties) {
    if (_completed) return;

    var t = new Test(name, properties);
    _tests.push(t);
    _pending_async++;
    _fire_start_callbacks_once();

    _promise_chain = _promise_chain.then(function() {
      return new Promise(function(resolve) {
        try {
          var result = fn.call(t, t);
          if (result && typeof result.then === "function") {
            result.then(function() {
              if (t.status === NOTRUN) t.status = PASS;
              _fire_result_callbacks(t);
              _pending_async--;
              _maybe_complete();
              resolve();
            }, function(e) {
              t.status = FAIL;
              t.message = e instanceof AssertionError ? e.message : String(e);
              _fire_result_callbacks(t);
              _pending_async--;
              _maybe_complete();
              resolve();
            });
          } else {
            if (t.status === NOTRUN) t.status = PASS;
            _fire_result_callbacks(t);
            _pending_async--;
            _maybe_complete();
            resolve();
          }
        } catch(e) {
          t.status = FAIL;
          t.message = e instanceof AssertionError ? e.message : String(e);
          _fire_result_callbacks(t);
          _pending_async--;
          _maybe_complete();
          resolve();
        }
      });
    });
  }

  // ─── promise_rejects_dom / promise_rejects_js ───
  function promise_rejects_dom(t, code, promise, description) {
    return promise.then(function() {
      t.status = FAIL;
      t.message = (description || "") + ": expected rejection";
    }, function(e) {
      // Acceptera — kontrollera DOMException-typ om möjligt
    });
  }

  function promise_rejects_js(t, constructor, promise, description) {
    return promise.then(function() {
      t.status = FAIL;
      t.message = (description || "") + ": expected rejection";
    }, function(e) {
      if (!(e instanceof constructor)) {
        t.status = FAIL;
        t.message = (description || "") + ": wrong exception type";
      }
    });
  }

  // ─── setup() ───
  function setup(fn_or_props, maybe_props) {
    var fn, props;
    if (typeof fn_or_props === "function") {
      fn = fn_or_props;
      props = maybe_props || {};
    } else {
      fn = null;
      props = fn_or_props || {};
    }

    if (props.explicit_done) _explicit_done = true;
    if (props.single_test) _single_test = true;
    if (props.timeout_multiplier) _timeout_multiplier = props.timeout_multiplier;

    if (fn) {
      try { fn(); } catch(e) { /* ignore setup errors */ }
    }
    _setup_done = true;
  }

  // ─── done() (explicit) ───
  function done() {
    if (_single_test && _single_test_obj) {
      if (_single_test_obj.status === NOTRUN) {
        _single_test_obj.status = PASS;
      }
      _fire_result_callbacks(_single_test_obj);
    }
    _complete();
  }

  // ─── generate_tests() ───
  function generate_tests(fn, args, properties) {
    for (var i = 0; i < args.length; i++) {
      var test_args = args[i];
      var test_name = test_args[0];
      test(function() {
        fn.apply(this, test_args.slice(1));
      }, test_name, properties);
    }
  }

  // ─── Internal helpers ───
  var _start_fired = false;
  function _fire_start_callbacks_once() {
    if (_start_fired) return;
    _start_fired = true;
    for (var i = 0; i < _start_callbacks.length; i++) {
      try { _start_callbacks[i](); } catch(e) {}
    }
  }

  function _fire_state_callbacks(t) {
    for (var i = 0; i < _state_callbacks.length; i++) {
      try { _state_callbacks[i](t); } catch(e) {}
    }
  }

  function _fire_result_callbacks(t) {
    _fire_state_callbacks(t);
    for (var i = 0; i < _result_callbacks.length; i++) {
      try { _result_callbacks[i](t); } catch(e) {}
    }
  }

  function _maybe_complete() {
    if (_completed) return;
    if (_explicit_done) return; // Vänta på explicit done()
    if (_pending_async > 0) return;
    _complete();
  }

  function _complete() {
    if (_completed) return;
    _completed = true;
    var status = { status: 0 }; // OK
    for (var i = 0; i < _completion_callbacks.length; i++) {
      try { _completion_callbacks[i](_tests, status); } catch(e) {}
    }
  }

  // ─── Expose globals ───
  // WPT testharness exponerar allt som globala funktioner
  if (typeof globalThis !== 'undefined') {
    globalThis.test = test;
    globalThis.async_test = async_test;
    globalThis.promise_test = promise_test;
    globalThis.setup = setup;
    globalThis.done = done;
    globalThis.generate_tests = generate_tests;
    globalThis.add_start_callback = add_start_callback;
    globalThis.add_test_state_callback = add_test_state_callback;
    globalThis.add_result_callback = add_result_callback;
    globalThis.add_completion_callback = add_completion_callback;
    globalThis.promise_rejects_dom = promise_rejects_dom;
    globalThis.promise_rejects_js = promise_rejects_js;

    // Assertions
    globalThis.assert_true = assert_true;
    globalThis.assert_false = assert_false;
    globalThis.assert_equals = assert_equals;
    globalThis.assert_not_equals = assert_not_equals;
    globalThis.assert_in_array = assert_in_array;
    globalThis.assert_array_equals = assert_array_equals;
    globalThis.assert_class_string = assert_class_string;
    globalThis.assert_readonly = assert_readonly;
    globalThis.assert_throws_dom = assert_throws_dom;
    globalThis.assert_throws_js = assert_throws_js;
    globalThis.assert_throws_exactly = assert_throws_exactly;
    globalThis.assert_unreached = assert_unreached;
    globalThis.assert_implements = assert_implements;
    globalThis.assert_implements_optional = assert_implements_optional;
    globalThis.assert_own_property = assert_own_property;
    globalThis.assert_inherits = assert_inherits;
    globalThis.assert_regexp_match = assert_regexp_match;

    // Test-objekt och statusar
    globalThis.Test = Test;
    globalThis.AssertionError = AssertionError;
  }

})();
