;;; kitty-graphics-test.scm — manual: render a red square via the Kitty protocol. Run: :scm (require "plugins/tests/kitty-graphics-test.scm")

(require "nothelix/graphics.scm")
(require "nothelix/string-utils.scm")

(provide run-kitty-graphics-test
         test-kitty-red-square)

(define *test-png-red-10x10*
  "iVBORw0KGgoAAAANSUhEUgAAAAoAAAAKCAIAAAACUFjqAAAADklEQVQY02P4z8DwHwUBABJIAfcWLqnCAAAAAElFTkSuQmCC")

(define (test-kitty-red-square)
  "Render a small red square using Kitty graphics protocol.
   If successful, a red square appears at the cursor position."
  (define image-id 9999)
  (define rows 3)
  (define escape-seq (kitty-display-image-bytes *test-png-red-10x10* image-id rows))
  (if (string-starts-with? escape-seq "ERROR:")
      (begin
        (set-status! (string-append "Kitty test failed: " escape-seq))
        #f)
      (begin
        (helix.static.insert_string "Kitty graphics test:\n")
        (helix.static.add-raw-content! escape-seq image-id rows (cursor-position))
        (helix.static.insert_string "\n[If you see a red square above, Kitty graphics works]\n")
        (set-status! "Kitty graphics test complete")
        #t)))

(define (run-kitty-graphics-test)
  "Run all Kitty graphics tests."
  (displayln "Running Kitty graphics protocol test...")
  (define result (test-kitty-red-square))
  (if result
      (displayln "PASS: Kitty graphics rendering")
      (displayln "FAIL: Kitty graphics rendering"))
  result)
