;;; project-config-test.scm — tests for the config key registry, application,
;;; and unknown-key reporting.

(require "test-framework.scm")
(require "../nothelix/project-config.scm")

(provide run-project-config-tests)

(define (run-project-config-tests)
  (reset-test-counters!)
  (print-test-suite-header "Project config tests")

  (assert-equal
    20
    (length (config-key-specs))
    "config-key-specs covers every supported key")

  (assert-equal
    '("nope" "also-nope")
    (unknown-config-keys '(("conceal-on-open" . #t)
                           ("nope" . 1)
                           ("julia-bin" . "/usr/bin/julia")
                           ("nope" . 2)
                           ("also-nope" . "x")))
    "unknown-config-keys reports each unknown key once, in file order")

  (define previous-conceal (conceal-on-open?))
  (apply-project-config! '(("conceal-on-open" . #f)))
  (assert-equal
    #f
    (conceal-on-open?)
    "apply-project-config! applies a boolean key")
  (apply-project-config! '(("conceal-on-open" . #t)))
  (assert-equal
    #t
    (conceal-on-open?)
    "apply-project-config! restores a boolean key")

  (assert-equal
    0
    (length (apply-project-config! '(("math-font-pt" . 16))))
    "apply-project-config! returns only unknown keys")

  (when (not previous-conceal)
    (apply-project-config! '(("conceal-on-open" . #f))))

  (assert-equal
    #t
    (let ([ref (config-keys-reference)])
      (and (> (string-length ref) 0)
           (string-contains? ref "render-cache")
           (string-contains? ref "conceal-on-open")))
    "config-keys-reference names every registered key")

  (print-test-suite-footer "Project config tests"))