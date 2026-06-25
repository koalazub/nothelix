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
(require "helix/components.scm")
(require-builtin helix/core/text as text.)
(require (prefix-in helix.static. "helix/static.scm"))
(require "helix/ext.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          getenv
                          render-math-to-svg
                          render-math-batch
                          math-image-grid
                          kitty-placeholder-payload
                          kitty-placeholder-rows))

(provide render-math-at-cursor
         render-all-display-math
         clear-math-images
         *math-image-target-rows*
         *math-image-cell-aspect*
         *math-image-pt-per-row*
         *math-image-text-color*
         *math-image-auto-color*
         *math-image-center*
         effective-math-text-color
         hex-luminance
         math-image-test-mode?
         set-math-image-test-mode!
         math-image-mock-result
         parse-math-image-result
         single-line-block-body
         display-math-block-ranges
         line-in-ranges?
         math-image-size
         math-block-image-id
         place-svg-image-at-line!)

;;; ---------------------------------------------------------------------------
;;; Configuration
;;; ---------------------------------------------------------------------------

;; Upper bound on the height of a rendered display-math image in terminal
;; rows. The actual row count scales with the equation's true height (see
;; the `math-image-grid` FFI / libnothelix math_image::math_image_grid);
;; this just caps a very tall equation so it can't eat the whole screen.
;; A `$$` block is the author emphasising a formula, so the cap is
;; generous — a multi-line derivation gets room to render large.
(define *math-image-target-rows* (box 18))

;; Typst font size in points passed to the Rust renderer. This sets the
;; intrinsic SVG size; on-screen size is governed by the placeholder grid.
(define *math-image-font-pt* (box 14))

;; Fallback hex colour (no #) for the rendered equation glyphs, used when
;; theme detection is off or the active theme's ui.text isn't a plain RGB
;; colour. Light grey reads on dark themes; set a dark hex for light ones.
(define *math-image-text-color* (box "e8e8e8"))

;; When #true, colour the glyphs with the active theme's `ui.text` (the
;; exact colour Helix draws normal text in) so the equation matches the
;; editor — light glyphs on a dark theme, dark on a light theme — without
;; any manual tuning. Falls back to *math-image-text-color* on failure.
(define *math-image-auto-color* (box #true))

;; Assumed terminal cell aspect ratio (cell-height / cell-width). Used
;; to map the SVG's intrinsic aspect ratio to terminal columns.
(define *math-image-cell-aspect* (box 2.0))

;; Points of equation height that map to one terminal row. Smaller values
;; render math larger (more rows per equation) — and since `cols` tracks
;; `rows`, lowering this is a uniform zoom that keeps the aspect ratio.
;; `$$` display math is meant to stand out, so this is tuned so a single
;; line renders ~3 rows tall (front-and-centre emphasis) rather than the
;; cramped ~2 a literal font-size mapping would give.
(define *math-image-pt-per-row* (box 7.0))

;; When #true, a display-math image is horizontally centered in the
;; focused view so an emphasized formula sits front-and-centre rather
;; than cramped against the left margin — matching how LaTeX, Pluto and
;; Jupyter present display math. Set #false to keep it left-aligned.
(define *math-image-center* (box #true))

;;; ---------------------------------------------------------------------------
;;; State
;;; ---------------------------------------------------------------------------

;; Map from doc-id to a hash of rendered math-block ids. Each block is
;; keyed by its anchor line index so re-renders replace in place rather
;; than stacking duplicates.
(define *math-image-registry* (hash))

;; Kitty placeholder image ids must stay < 2^24 — the fork carries the id
;; in a 24-bit foreground colour (document.rs id_24 = raw.id & 0xFFFFFF),
;; so any id above 16,777,215 references a never-transmitted image and the
;; cells render blank. Disjoint bands under that ceiling: plots 1000+,
;; paths 1M, tables 4M, math 8M.
(define *math-image-id-base* 8000000)

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

(define (math-image-mock-result)
  *math-image-test-result*)

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

;; Compute terminal (rows . cols) for a display-math image. The
;; deterministic computation — rows scale with the equation's true
;; height, cols chosen so the on-screen aspect ratio matches the SVG —
;; lives in tested Rust (libnothelix math_image::math_image_grid, exposed
;; as `math-image-grid`). This is a thin wrapper that passes the
;; runtime-tunable config through and parses the `"rows,cols"` reply.
;; cell-aspect / pt-per-row cross the dylib FFI as strings (the Steel
;; dylib boundary has no float-argument marshaller); the Rust side parses
;; them back to f64.
(define (math-image-size width height max-rows aspect)
  (define reply (math-image-grid width height max-rows
                                 (number->string aspect)
                                 (number->string (unbox *math-image-pt-per-row*))))
  (define parts (string-split reply ","))
  (if (= (length parts) 2)
      (cons (string->number (car parts)) (string->number (cadr parts)))
      ;; Defensive: a malformed reply falls back to a minimum readable grid
      ;; rather than crashing the render path.
      (cons 2 10)))

;; Columns to shift a `cols`-wide image so it sits centered in the
;; focused view. Zero when centering is off, the view area is
;; unavailable, or the image is already as wide as the view.
(define (math-image-center-offset cols)
  (if (unbox *math-image-center*)
      (let ([area (editor-focused-buffer-area)])
        (if area
            (quotient (max 0 (- (area-width area) cols)) 2)
            0))
      0))

;; Left-pad every placeholder row by `offset` blank cells. The fork
;; draws each row starting at the anchor's column (the line's left edge
;; for a `# $$` block), so leading spaces shift the whole image right
;; without moving the buffer anchor — that's what centers it.
(define (math-image-indent-rows placeholder-rows offset)
  (if (<= offset 0)
      placeholder-rows
      (let ([pad (make-string offset #\space)])
        (string-join
          (map (lambda (row) (string-append pad row))
               (string-split placeholder-rows "\n"))
          "\n"))))

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

;; List of (open-line . close-line) for every display-math block in the
;; buffer — multi-line `# $$ ... # $$` and single-line `# $$ x $$`
;; (close = open). The other renderers (inline conceal, stacked rows)
;; consult this to leave these lines to the image renderer.
(define (display-math-block-ranges rope total-lines)
  (define (line-body idx)
    (and (>= idx 0) (< idx total-lines)
         (let* ([s (text.rope->string (text.rope->line rope idx))]
                [t (if (string-suffix? s "\n")
                       (substring s 0 (- (string-length s) 1))
                       s)])
           (and (string-starts-with? t "# ")
                (substring t 2 (string-length t))))))
  (define (find-close idx)
    (let scan ([j (+ idx 1)])
      (cond
        [(>= j total-lines) #false]
        [(equal? (line-body j) "$$") j]
        [(line-body j) (scan (+ j 1))]
        [else #false])))
  (let loop ([idx 0] [ranges '()])
    (cond
      [(>= idx total-lines) (reverse ranges)]
      [else
       (define body (line-body idx))
       (cond
         [(equal? body "$$")
          (define close (find-close idx))
          (if close
              (loop (+ close 1) (cons (cons idx close) ranges))
              (loop (+ idx 1) ranges))]
         [(and body (single-line-block-body body))
          (loop (+ idx 1) (cons (cons idx idx) ranges))]
         [else (loop (+ idx 1) ranges)])])))

(define (line-in-ranges? line ranges)
  (cond
    [(null? ranges) #false]
    [(let ([r (car ranges)]) (and (>= line (car r)) (<= line (cdr r)))) #true]
    [else (line-in-ranges? line (cdr ranges))]))

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
;; Format a 0-255 channel as two lowercase hex digits.
(define *hex-digits* "0123456789abcdef")
(define (byte->hex2 n)
  (let ([v (max 0 (min 255 n))])
    (string-append
      (make-string 1 (string-ref *hex-digits* (quotient v 16)))
      (make-string 1 (string-ref *hex-digits* (modulo v 16))))))

;; "rrggbb" for a Color, or #false unless it's a plain RGB colour
;; (Color-red/green/blue yield #false for named/indexed colours).
(define (color->hex color)
  (let ([r (and color (Color-red color))]
        [g (and color (Color-green color))]
        [b (and color (Color-blue color))])
    (and (number? r) (number? g) (number? b)
         (string-append (byte->hex2 r) (byte->hex2 g) (byte->hex2 b)))))

(define (theme-scope-hex get-style get-color)
  (with-handler
    (lambda (_) #false)
    (let ([style (get-style)])
      (and style (color->hex (get-color style))))))

(define (hex2->byte s)
  (define (digit c)
    (let ([v (char->integer c)])
      (cond
        [(and (>= v 48) (<= v 57)) (- v 48)]
        [(and (>= v 97) (<= v 102)) (+ 10 (- v 97))]
        [(and (>= v 65) (<= v 70)) (+ 10 (- v 65))]
        [else 0])))
  (+ (* 16 (digit (string-ref s 0))) (digit (string-ref s 1))))

(define (hex-luminance hex)
  (quotient (+ (* 299 (hex2->byte (substring hex 0 2)))
               (* 587 (hex2->byte (substring hex 2 4)))
               (* 114 (hex2->byte (substring hex 4 6))))
            1000))

;; ui.text when it's a plain RGB colour; otherwise a glyph colour chosen to
;; contrast with ui.background's luminance, so a light theme renders dark
;; glyphs instead of the near-white default that washes out on cream.
(define (effective-math-text-color)
  (cond
    [(not (unbox *math-image-auto-color*)) (unbox *math-image-text-color*)]
    [(theme-scope-hex theme->fg style->fg)]
    [else
     (let ([bg (theme-scope-hex theme->bg style->bg)])
       (cond
         [(and bg (> (hex-luminance bg) 140)) "1a1a1a"]
         [bg "e8e8e8"]
         [else (unbox *math-image-text-color*)]))]))

(define (call-render-math-to-svg latex font-pt)
  (if (math-image-test-mode?)
      *math-image-test-result*
      (render-math-to-svg latex font-pt (effective-math-text-color))))

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
             8000000)))

(define (place-svg-image-at-line! result-json image-id rope anchor-line label)
  (define result (parse-math-image-result result-json))
  (cond
    [(not result)
     (debug-log (string-append label ": parse failure for result: " result-json))
     #false]
    [(> (string-length (list-ref result 3)) 0)
     (debug-log (string-append label ": typst error: " (list-ref result 3)))
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
        (debug-log (string-append label ": payload error: " payload))
        #false]
       [(= (string-length placeholder-rows) 0)
        (debug-log (string-append label ": placeholder grid too large"))
        #false]
       [else
         (define char-idx (text.rope-line->char rope anchor-line))
         (define offset (math-image-center-offset cols))
         (define centered-rows (math-image-indent-rows placeholder-rows offset))
         (call-add-raw-content-with-placeholders! payload rows cols centered-rows char-idx)
         (debug-log
           (string-append label ": registered id=" (number->string image-id)
                          " anchor=" (number->string anchor-line)
                          " rows=" (number->string rows)
                          " cols=" (number->string cols)
                          " offset=" (number->string offset)))
         #true])]))

;; Render a display math block and register it as RawContent. Returns
;; #true on success, #false on failure.
(define (render-display-math-block rope total-lines anchor-line content-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define latex (string-join content-lines "\n"))
  (define image-id (math-block-image-id doc-id anchor-line latex))
  (define result-json (call-render-math-to-svg latex (unbox *math-image-font-pt*)))
  (place-svg-image-at-line! result-json image-id rope anchor-line "math-image"))

;; Record-separator delimiter framing the batch FFI (matches Rust BATCH_SEP).
(define *math-batch-sep* (make-string 1 (integer->char 30)))

;; MAIN THREAD: every (anchor-line . latex) for the buffer's display math.
(define (collect-math-jobs rope total-lines)
  (let loop ([ranges (display-math-block-ranges rope total-lines)] [acc '()])
    (if (null? ranges)
        (reverse acc)
        (let ([block (find-display-math-block rope total-lines (car (car ranges)))])
          (loop (cdr ranges)
                (if block
                    (cons (cons (car block) (string-join (cdr block) "\n")) acc)
                    acc))))))

;; MAIN THREAD: place one already-rendered block.
(define (place-math-job rope doc-id job result-json)
  (define anchor (car job))
  (define image-id (math-block-image-id doc-id anchor (cdr job)))
  (place-svg-image-at-line! result-json image-id rope anchor "math-image"))

(define (block-count-phrase n)
  (string-append (number->string n) " block" (if (= n 1) "" "s")))

;; Compile every block on a background thread (Typst runs in parallel via
;; render-math-batch), then register the images back on the main thread, so
;; the editor stays live while a notebook's equations rasterise.
(define (render-display-math-async jobs doc-id)
  (define color (effective-math-text-color))
  (define font-pt (unbox *math-image-font-pt*))
  (define blob (string-join (map cdr jobs) *math-batch-sep*))
  (set-status! (string-append "math-image: rendering " (block-count-phrase (length jobs)) "…"))
  (spawn-native-thread
    (lambda ()
      (define results (string-split (render-math-batch blob font-pt color) *math-batch-sep*))
      (hx.with-context
        (lambda ()
          (define rope (editor->text doc-id))
          (let place ([js jobs] [rs results] [placed 0])
            (if (or (null? js) (null? rs))
                (set-status! (string-append "math-image: rendered " (block-count-phrase placed)))
                (place (cdr js) (cdr rs)
                       (if (place-math-job rope doc-id (car js) (car rs))
                           (+ placed 1)
                           placed)))))))))

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
     (define jobs (collect-math-jobs rope total-lines))
     (cond
       [(null? jobs)
        (set-status! "math-image: no display math to render")]
       ;; Test mode renders synchronously so suites stay deterministic; the
       ;; mock FFI returns instantly and never touches a real thread.
       [(math-image-test-mode?)
        (for-each
          (lambda (job)
            (place-math-job rope doc-id job
                            (call-render-math-to-svg (cdr job) (unbox *math-image-font-pt*))))
          jobs)
        (set-status! (string-append "math-image: rendered " (block-count-phrase (length jobs))))]
       [else
        (render-display-math-async jobs doc-id)])]))

;;@doc
;; Remove all math-image RawContent entries from the current view.
;; Best-effort: requires the fork's clear-raw-content! binding.
(define (clear-math-images)
  (maybe-clear-raw-content!)
  (set-status! "math-image: cleared"))
