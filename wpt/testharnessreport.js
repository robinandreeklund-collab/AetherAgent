/*
 * AetherAgent WPT Test Harness Reporter
 *
 * Baserad på lightpandas approach. Samlar testresultat i ett globalt
 * `report`-objekt som WPT-runnern läser efter exekvering.
 *
 * Format: "test_name|status|message" per rad i report.log
 *
 * Referens: https://web-platform-tests.org/writing-tests/testharness-api.html#callback-api
 */
var report = {
  complete: false,
  status: "",
  log: "no test suite completion|Fail|The test never reaches the completion callback.",
  cases: {},
  passed: 0,
  failed: 0,
  timedout: 0,
  notrun: 0,
  name: function(test) {
    var n = test.name;
    return n ? n.replace(/\n/g, '').replace(/\|/g, '_') : "(unnamed)";
  },
  format: function(test) {
    var log = report.name(test) + "|" + test.format_status();
    if (test.message != null) {
      log += "|" + test.message.replaceAll("\n", " ");
    }
    return log;
  }
};

function update() {
  var log = "";
  Object.keys(report.cases).forEach(function(k) {
    log += report.cases[k] + "\n";
  });
  report.log = log;
}

add_test_state_callback(function(test) {
  report.cases[report.name(test)] = report.format(test);
  update();
});

add_result_callback(function(test) {
  report.cases[report.name(test)] = report.format(test);
  if (test.status === test.PASS) report.passed++;
  else if (test.status === test.FAIL) report.failed++;
  else if (test.status === test.TIMEOUT) report.timedout++;
  else if (test.status === test.NOTRUN) report.notrun++;
  update();
});

add_completion_callback(function(tests, status) {
  report.complete = true;
  report.status = status.status === 0 ? "OK" : "ERROR";
});
