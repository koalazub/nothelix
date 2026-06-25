;;; math-image.scm - Render LaTeX display math as inline SVG images.
;;;
;;; Uses Typst to typeset the math and the Kitty Unicode placeholder
;;; protocol to embed the resulting image inside the Helix buffer. SVG
;;; export keeps math resolution-independent; the Kitty payload path
;;; rasterizes at display time so scaling is not tied to a guessed PPI.

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
(require "json-utils.scm")
(require "image-cache.scm")
(require "conceal.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          getenv
                          render-math-to-svg
                          kitty-placeholder-payload
                          kitty-placeholder-rows))

(provide render-math-at-cursor
         render-all-display-math
         clear-math-images
         *math-image-target-rows*
         *math-image-cell-aspect*
         math-image-test-mode?
         set-math-image-test-mode!
         parse-math-image-result
         single-line-block-body
         math-image-size
         math-block-image-id)

;;; ---------------------------------------------------------------------------
;;; Configuration
;;; ---------------------------------------------------------------------------

;; Target height of a rendered display-math image in terminal rows.
;; Larger values make complex equations easier to read but consume more
;; vertical space.
(define *math-image-target-rows* (box 5))

;; Typst font size in points passed to the Rust renderer. This sets the
;; intrinsic SVG size; on-screen size is governed by the placeholder grid.
(define *math-image-font-pt* (box 14))

;; Assumed terminal cell aspect ratio (cell-height / cell-width). Used
;; to map the SVG's intrinsic aspect ratio to terminal columns.
(define *math-image-cell-aspect* (box 2.0))

;;; ---------------------------------------------------------------------------
;;; State
;;; ---------------------------------------------------------------------------

;; Map from doc-id to a hash of rendered math-block ids. Each block is
;; keyed by its anchor line index so re-renders replace in place rather
;; than stacking duplicates.
(define *math-image-registry* (hash))

;; Image id range disjoint from cell plots (1000+) and arbitrary paths
;; (2_000_000+). Display math images start at 3_000_000.
(define *math-image-id-base* 3000000)

;;; ---------------------------------------------------------------------------
;;; Test mode
;;; ---------------------------------------------------------------------------

;; When #true, image rendering is mocked: Typst is not invoked and no
;; Kitty/RawContent payload is emitted. This lets the test suite exercise
;; the full detection/sizing pipeline without garbling the terminal with
;; binary image data.
;;
;; Test mode is auto-enabled when the NOTHELIX_TEST environment variable
;; is set at plugin load time, and can be toggled at runtime with
;; `set-math-image-test-mode!` (used by `:run-all-tests`).
(define *math-image-test-mode* (box (not (string=? "" (getenv "NOTHELIX_TEST")))))

(define (math-image-test-mode?)
  (unbox *math-image-test-mode*))

(define (set-math-image-test-mode! val)
  (set-box! *math-image-test-mode* val))

;; Mock JSON emitted by render-math-to-svg in test mode. Uses a 160x80
;; rectangle so the sizing math is still exercised.
(define *math-image-test-result*
  "{\"b64\":\"PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxNjAiIGhlaWdodD0iODAiIHZpZXdCb3g9IjAgMCAxNjAgODAiPjxyZWN0IHdpZHRoPSIxNjAiIGhlaWdodD0iODAiIGZpbGw9IndoaXRlIi8+PC9zdmc+\",\"width\":160,\"height\":80,\"error\":\"\"}")

;;; ---------------------------------------------------------------------------
;;; JSON result parsing
;;; ---------------------------------------------------------------------------

;; Parse the JSON object emitted by render-math-to-svg:
;;   {"b64":"...","width":W,"height":H,"error":""}
;; Returns a list (b64 width height error) or #false on parse failure.
(define (parse-math-image-result json)
  (with-handler
    (lambda (_) #false)
    (let* ([b64-start (json-find-char json #\" 0)]
           [_ (unless b64-start (error "no b64 key"))]
           [b64-quote (+ b64-start 6)]
           [b64-end (json-find-string-end json (+ b64-quote 1))]
           [b64 (json-extract-string json (+ b64-quote 1))]
           [width-start (json-find-char json #\: (+ b64-end 1))]
           [width-end (json-find-non-digit json (+ width-start 1))]
           [width (string->number (substring json (+ width-start 1) width-end))]
           [height-start (json-find-char json #\: width-end)]
           [height-end (json-find-non-digit json (+ height-start 1))]
           [height (string->number (substring json (+ height-start 1) height-end))]
           [err-start (json-find-char json #\" height-end)]
           [err-quote (+ err-start 8)]
           [err-end (json-find-string-end json (+ err-quote 1))]
           [err (json-extract-string json (+ err-quote 1))])
      (list b64 width height err))))

;;; ---------------------------------------------------------------------------
;;; Sizing
;;; ---------------------------------------------------------------------------

;; Compute terminal rows/cols from pixel dimensions and configured target.
(define (math-image-size width height target-rows aspect)
  (define pixel-row-height (/ height target-rows))
  (define rows (exact (max 2 (floor (+ 0.5 (/ height pixel-row-height))))))
  (define cols (exact (max 10 (floor (+ 0.5 (* rows (/ width height) aspect))))))
  (cons rows cols))

;;; ---------------------------------------------------------------------------
;;; Block detection
;;; ---------------------------------------------------------------------------

;; Extract the inner content of a single-line `# $$ ... $$` comment,
;; or #false if the line is not a single-line display math block.
(define (single-line-block-body body)
  (and (string-starts-with? body "$$")
       (string-suffix? body "$$")
       (> (string-length body) 4)
       (string-trim (substring body 2 (- (string-length body) 2)))))

;; Find the display math block surrounding `line-idx`. Returns a pair
;; (anchor-line . content-lines) or #false.  Only scans Julia comment
;; lines (# ...) because .jl notebooks carry markdown cells as comments.
(define (find-display-math-block rope total-lines line-idx)
  (define (line-content idx)
    (and (>= idx 0) (< idx total-lines)
         (let ([s (text.rope->string (text.rope->line rope idx))])
           (if (string-suffix? s "\n")
               (substring s 0 (- (string-length s) 1))
               s))))
  (define (comment-body s)
    (and (string-starts-with? s "# ")
         (substring s 2 (string-length s))))

  ;; Walk up to find the nearest "# $$" opener.
  (let search-up ([idx line-idx])
    (cond
      [(< idx 0) #false]
      [else
       (define raw (line-content idx))
       (define body (comment-body raw))
       (cond
         [(equal? body "$$")
          (collect-block rope total-lines idx line-content comment-body)]
         [(single-line-block-body body)
          => (lambda (inner)
               (cons idx (list inner)))]
         [body (search-up (- idx 1))]
         [else #false])])))

(define (collect-block rope total-lines opener-line line-content comment-body)
  (define content-lines '())
  (let search-down ([idx (+ opener-line 1)])
    (cond
      [(>= idx total-lines) #false]
      [else
       (define raw (line-content idx))
       (define body (comment-body raw))
       (cond
         [(equal? body "$$")
          (cons opener-line (reverse content-lines))]
         [body
          (set! content-lines (cons body content-lines))
          (search-down (+ idx 1))]
         [else #false])])))

;;; ---------------------------------------------------------------------------
;;; FFI wrappers (mockable in test mode)
;;; ---------------------------------------------------------------------------

;; Wrapper around render-math-to-svg. In test mode returns a deterministic
;; mock result so the sizing path is exercised without invoking Typst.
(define (call-render-math-to-svg latex font-pt)
  (if (math-image-test-mode?)
      *math-image-test-result*
      (render-math-to-svg latex font-pt)))

;; Wrapper around the fork's RawContent registration. In test mode we
;; skip the actual terminal payload entirely, which prevents binary Kitty
;; graphics data from polluting captured test output.
(define (call-add-raw-content-with-placeholders! payload rows cols placeholder-rows char-idx)
  (if (math-image-test-mode?)
      (begin
        (debug-log
          (string-append "math-image-test: would register id placeholder rows="
                         (number->string rows) " cols=" (number->string cols)
                         " char=" (number->string char-idx)))
        #true)
      (begin
        (helix.static.add-raw-content-with-placeholders! payload rows cols placeholder-rows char-idx)
        #true)))

;;; ---------------------------------------------------------------------------
;;; Rendering a single block
;;; ---------------------------------------------------------------------------

;; djb2 hash used for the block id.  Same algorithm as image-cache.scm.
(define (math-block-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

;; Stable image id for a math block at `anchor-line` in `doc-id`.
(define (math-block-image-id doc-id anchor-line latex)
  (+ *math-image-id-base*
     (modulo (+ (math-block-hash (number->string anchor-line))
                (math-block-hash latex))
             100000000)))

;; Render a display math block and register it as RawContent. Returns
;; #true on success, #false on failure.
(define (render-display-math-block rope total-lines anchor-line content-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define latex (string-join content-lines "\n"))
  (define image-id (math-block-image-id doc-id anchor-line latex))
  (define result-json (call-render-math-to-svg latex (unbox *math-image-font-pt*)))
  (define result (parse-math-image-result result-json))

  (cond
    [(not result)
     (debug-log (string-append "math-image: parse failure for result: " result-json))
     #false]
    [(> (string-length (list-ref result 3)) 0)
     (debug-log (string-append "math-image: typst error: " (list-ref result 3)))
     #false]
    [else
     (define b64 (list-ref result 0))
     (define width (list-ref result 1))
     (define height (list-ref result 2))
     (define size (math-image-size width height (unbox *math-image-target-rows*) (unbox *math-image-cell-aspect*)))
     (define rows (car size))
     (define cols (cdr size))
     (define payload (kitty-placeholder-payload b64 image-id))
     (define placeholder-rows (kitty-placeholder-rows image-id cols rows))

     (cond
       [(string-starts-with? payload "ERROR:")
        (debug-log (string-append "math-image: payload error: " payload))
        #false]
       [(= (string-length placeholder-rows) 0)
        (debug-log "math-image: placeholder grid too large")
        #false]
       [else
         (define char-idx (text.rope-line->char rope anchor-line))
         (call-add-raw-content-with-placeholders! payload rows cols placeholder-rows char-idx)
         (debug-log
           (string-append "math-image: registered id=" (number->string image-id)
                          " anchor=" (number->string anchor-line)
                          " rows=" (number->string rows)
                          " cols=" (number->string cols)))
         #true])]))

;;; ---------------------------------------------------------------------------
;;; Public commands
;;; ---------------------------------------------------------------------------

;;@doc
;; Render the display math block under the cursor as a Typst-typeset
;; SVG image.
(define (render-math-at-cursor)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define rope (editor->text doc-id))
  (define total-lines (text.rope-len-lines rope))
  (define line-idx (text.rope-char->line rope (cursor-position)))
  (define block (find-display-math-block rope total-lines line-idx))
  (cond
    [(not block)
     (set-status! "math-image: cursor is not inside a # $$ ... # $$ display math block")]
    [else
     (define anchor-line (car block))
     (define content-lines (cdr block))
     (if (render-display-math-block rope total-lines anchor-line content-lines)
         (set-status! "math-image: rendered display math")
         (set-status! "math-image: render failed (see log)"))]))

;;@doc
;; Scan the current buffer for every `# $$ ... # $$` display math block
;; and render each one as an image. Only runs on files that may contain
;; LaTeX math (md/tex/jl/typ etc.) to avoid pointless scans of huge
;; source files.
(define (render-all-display-math)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (cond
    [(not (file-has-conceal-extension? path))
     (set-status! "math-image: not a math-enabled file type")]
    [else
     (define rope (editor->text doc-id))
     (define total-lines (text.rope-len-lines rope))
     (define rendered 0)
     (define failed 0)

     (let loop ([idx 0])
       (when (< idx total-lines)
         (define block (find-display-math-block rope total-lines idx))
         (cond
           [block
            (define anchor-line (car block))
            (define content-lines (cdr block))
            (if (render-display-math-block rope total-lines anchor-line content-lines)
                (set! rendered (+ rendered 1))
                (set! failed (+ failed 1)))
            ;; Skip past the block so we don't re-render its inner lines.
            (loop (+ anchor-line (length content-lines) 2))]
           [else (loop (+ idx 1))])))

     (set-status!
       (string-append "math-image: rendered " (number->string rendered)
                      " block" (if (= rendered 1) "" "s")
                      (if (> failed 0)
                          (string-append ", " (number->string failed) " failed")
                          "")))]))

;;@doc
;; Remove all math-image RawContent entries from the current view.
;; Best-effort: requires the fork's clear-raw-content! binding.
(define (clear-math-images)
  (maybe-clear-raw-content!)
  (set-status! "math-image: cleared"))
