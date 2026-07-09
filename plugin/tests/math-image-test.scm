;;; math-image-test.scm — tests for the math-image JSON parser, block detection, and sizing.

(require "test-framework.scm")
(require "../nothelix/string-utils.scm")
(require "../nothelix/math-image.scm")

(provide run-math-image-tests)

(define (make-rope lines)
  (list->vector lines))

(define (rope-line rope idx)
  (vector-ref rope idx))

(define (test-find-block lines line-idx)
  (define rope (make-rope lines))
  (define (line-content idx)
    (and (>= idx 0) (< idx (vector-length rope))
         (let ([s (rope-line rope idx)])
           (if (string-suffix? s "\n")
               (substring s 0 (- (string-length s) 1))
               s))))
  (define (comment-body s)
    (and (string-starts-with? s "# ")
         (substring s 2 (string-length s))))
  (let search-up ([idx line-idx])
    (cond
      [(< idx 0) #false]
      [else
       (define raw (line-content idx))
       (define body (comment-body raw))
       (cond
         [(equal? body "$$")
          (collect-block rope idx line-content comment-body)]
         [(single-line-block-body body)
          => (lambda (inner) (cons idx (list inner)))]
         [body (search-up (- idx 1))]
         [else #false])])))

(define (collect-block rope opener-line line-content comment-body)
  (define content-lines '())
  (let search-down ([idx (+ opener-line 1)])
    (cond
      [(>= idx (vector-length rope)) #false]
      [else
       (define raw (line-content idx))
       (define body (comment-body raw))
       (cond
         [(equal? body "$$") (cons opener-line (reverse content-lines))]
         [body
          (set! content-lines (cons body content-lines))
          (search-down (+ idx 1))]
         [else #false])])))

(define (run-math-image-tests)
  (reset-test-counters!)
  (print-test-suite-header "Math image tests")

  (assert-equal
    (list "PHN2ZyB3aWR0aD0iMTUwIiBoZWlnaHQ9IjUwIj48L3N2Zz4=" 150 50 "")
    (parse-math-image-result
      "{\"b64\":\"PHN2ZyB3aWR0aD0iMTUwIiBoZWlnaHQ9IjUwIj48L3N2Zz4=\",\"width\":150,\"height\":50,\"error\":\"\"}")
    "parse-math-image-result extracts b64, width, height, error")

  (assert-equal
    (list "" 0 0 "typst compile failed")
    (parse-math-image-result
      "{\"b64\":\"\",\"width\":0,\"height\":0,\"error\":\"typst compile failed\"}")
    "parse-math-image-result handles empty b64 and error")

  (let* ([big-b64 (make-string 50000 #\A)]
         [json (string-append "{\"b64\":\"" big-b64
                              "\",\"width\":1280,\"height\":640,\"error\":\"\"}")])
    (assert-equal
      (list big-b64 1280 640 "")
      (parse-math-image-result json)
      "parse-math-image-result handles a tens-of-KB base64 payload"))

  (assert-equal
    "x = 1"
    (single-line-block-body "$$ x = 1 $$")
    "single-line-block-body extracts inner math")

  (assert-equal
    #f
    (single-line-block-body "$$")
    "single-line-block-body rejects opener-only")

  (assert-equal
    #f
    (single-line-block-body "# not math")
    "single-line-block-body rejects plain comment")

  (assert-equal
    (cons 1 (list "x = 1" "y = 2"))
    (test-find-block
      (list "# unrelated"
            "# $$"
            "# x = 1"
            "# y = 2"
            "# $$"
            "# after")
      2)
    "find-display-math-block locates multi-line block from inside")

  (assert-equal
    #f
    (test-find-block
      (list "# $$"
            "# x = 1")
      1)
    "find-display-math-block rejects unclosed block")

  (assert-equal
    (cons 5 20)
    (math-image-size 160 80 5 2.0)
    "math-image-size maps 160x80 to 5 rows x 20 cols")

  (assert-equal
    (math-block-image-id 'doc-a 5 "x = 1")
    (math-block-image-id 'doc-a 5 "x = 1")
    "math-block-image-id is stable for identical input")

  (assert-equal
    #f
    (equal? (math-block-image-id 'doc-a 5 "x = 1")
            (math-block-image-id 'doc-a 5 "x = 2"))
    "math-block-image-id differs for different latex")

  (print-test-suite-footer "Math image tests"))
