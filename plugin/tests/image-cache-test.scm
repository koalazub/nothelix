;;; image-cache-test.scm — boundary tests for image-id config/derivation, pinning the
;;; exact-integer? guard against untrusted-config float injection (`plots-per-cell = 32.0`),
;;; plus insert-image argument resolution and paste-image routing.

(require "test-framework.scm")
(require "../nothelix/common.scm")
(require "../nothelix/image-cache.scm")

(provide %run-image-cache-tests)

(define (%run-image-cache-tests)
  (%reset-test-counters!)
  (%print-test-suite-header "image-cache")

  (define saved-test-mode (%nothelix-test-mode?))
  (%set-nothelix-test-mode! #t)

  (define default-ppc (%plots-per-cell))

  (%set-plots-per-cell! 32.5)
  (%assert-equal default-ppc (%plots-per-cell) "set-plots-per-cell! rejects 32.5 (not exact-integer?)")

  (%set-plots-per-cell! 32.0)
  (%assert-equal default-ppc (%plots-per-cell) "set-plots-per-cell! rejects 32.0 (not exact-integer?)")

  (%set-plots-per-cell! -5)
  (%assert-equal default-ppc (%plots-per-cell) "set-plots-per-cell! rejects -5 (not > 0)")

  (%set-plots-per-cell! 0)
  (%assert-equal default-ppc (%plots-per-cell) "set-plots-per-cell! rejects 0 (not > 0)")

  (%set-plots-per-cell! 32)
  (%assert-equal 32 (%plots-per-cell) "set-plots-per-cell! accepts exact-integer 32")

  (%set-plots-per-cell! 300)
  (%assert-equal 256 (%plots-per-cell) "set-plots-per-cell! clamps 300 to 256")

  (%set-plots-per-cell! default-ppc)

  (define (id-ok? cell img)
    (define id (%cell-img->image-id cell img))
    (and (exact-integer? id) (< id 4000000)))

  (%assert-true (id-ok? 0 0) "cell-img->image-id (0,0) is an exact integer under 4M")
  (%assert-true (id-ok? 5 3) "cell-img->image-id (5,3) is an exact integer under 4M")
  (%assert-true (id-ok? 100 5) "cell-img->image-id (100,5) is an exact integer under 4M")
  (%assert-true (id-ok? 99999 999) "cell-img->image-id (99999,999) is an exact integer under 4M")
  (%assert-true (id-ok? 0 -1) "cell-img->image-id tolerates a negative img-index")

  (%assert-equal (cons 3 1) (%extract-cell-and-img-from-path "cell-3-1.png")
                "extract-cell-and-img-from-path parses cell-3-1.png -> (3 . 1)")
  (%assert-equal (cons 42 0) (%extract-cell-and-img-from-path "cell-42.png")
                "extract-cell-and-img-from-path parses legacy cell-42.png -> (42 . 0)")
  (%assert-equal #false (%extract-cell-and-img-from-path "notacell.png")
                "extract-cell-and-img-from-path rejects a non-matching path")

  (%assert-equal 'prompt (%insert-image-action '())
                "insert-image-action: zero arguments resolves to 'prompt, not an error")
  (%assert-equal "plot.png" (%insert-image-action '("plot.png"))
                "insert-image-action: a path argument resolves to the path")
  (%assert-equal "plot.png" (%insert-image-action '("  plot.png  "))
                "insert-image-action: trims surrounding whitespace")
  (%assert-equal #false (%insert-image-action '(""))
                "insert-image-action: an explicitly empty path resolves to #false")
  (%assert-equal #false (%insert-image-action '("   "))
                "insert-image-action: a whitespace-only path resolves to #false")
  (%assert-true (with-handler (lambda (_) #false) (begin (insert-image) #true))
               "insert-image invoked with zero arguments does not raise ArityMismatch")

  (%assert-true (%paste-image-eligible? "/tmp/nb.jl" #false)
               "paste-image-eligible? accepts a .jl path outside test mode")
  (%assert-false (%paste-image-eligible? "/tmp/nb.jl" #true)
                "paste-image-eligible? refuses a .jl path in test mode")
  (%assert-false (%paste-image-eligible? "/tmp/notes.md" #false)
                "paste-image-eligible? refuses a non-.jl path")
  (%assert-false (%paste-image-eligible? #false #false)
                "paste-image-eligible? refuses a missing path")
  (%assert-equal (cons 'insert ".nothelix/images/paste-7.png")
                (%paste-image-result->plan ".nothelix/images/paste-7.png")
                "paste-image-result->plan routes a sidecar path to insert")
  (%assert-equal (cons '%status "paste-image: ERROR: clipboard: no image on the clipboard")
                (%paste-image-result->plan "ERROR: clipboard: no image on the clipboard")
                "paste-image-result->plan routes an ERROR reply to a status message")
  (%assert-true (with-handler (lambda (_) #false) (begin (paste-image) #true))
                "paste-image invoked in test mode does not raise and does not touch the buffer")
  (%assert-equal (cons 1920 1080) (%parse-dims-wxh "1920x1080")
                "parse-dims-wxh reads a WxH reply")
  (%assert-equal #false (%parse-dims-wxh "ERROR: cannot read /tmp/x.png: denied")
                "parse-dims-wxh rejects an ERROR reply")
  (%assert-equal #false (%parse-dims-wxh "no-separator")
                "parse-dims-wxh rejects a reply without a separator")
  (%assert-equal (cons 76 51) (%parse-grid-cols-rows "76,51")
                "parse-grid-cols-rows reads a cols,rows reply")
  (%assert-equal #false (%parse-grid-cols-rows "ERROR: boom")
                "parse-grid-cols-rows rejects an ERROR reply")
  (%assert-equal (cons 40 12) (%clamp-plot-grid (cons 40 12) 80 60)
                "clamp-plot-grid keeps a fitting grid untouched")
  (%assert-equal (cons 76 12) (%clamp-plot-grid (cons 200 12) 80 60)
                "clamp-plot-grid clamps wide columns into the viewport")
  (%assert-equal (cons 40 36) (%clamp-plot-grid (cons 40 200) 80 36)
                "clamp-plot-grid clamps tall rows into the row budget")

  (%set-nothelix-test-mode! saved-test-mode)

  (%print-test-suite-footer "image-cache"))
