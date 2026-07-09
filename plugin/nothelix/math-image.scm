;;; math-image.scm - Render LaTeX display math as inline SVG images.

(require "common.scm")
(require "debug.scm")
(require "string-utils.scm")
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
                          start-render-batch
                          poll-render-batch
                          math-image-grid
                          math-block-latex-batch
                          reserve-math-lines
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
         place-svg-image-at-line!
         math-image-mask-width
         set-math-image-font-pt!
         set-math-image-color!
         set-math-image-width-override!)

;; Configuration
(define *math-image-target-rows* (box 18))
(define *math-image-font-pt* (box 14))
(define *math-image-text-color* (box "e8e8e8"))
(define *math-image-auto-color* (box #true))
(define *math-image-cell-aspect* (box 2.0))
(define *math-image-pt-per-row* (box 7.0))
(define *math-image-center* (box #true))
(define *math-image-side-margin* (box 4))
;; #false = derive render width from the view; a positive number pins it
;; (set from a project's .nothelix.scm via set-math-image-width-override!).
(define *math-image-width-override* (box #false))

;; State
(define *math-image-registry* (hash))

;; Ids must stay < 2^24; bands: plots 1000+, paths 1M, tables 4M, math 8M.
(define *math-image-id-base* 8000000)
(define *math-image-id-limit* (+ *math-image-id-base* 8000000))

(define (try-clear-math-image-band!)
  (with-handler
    (lambda (_) #false)
    (eval `(helix.static.clear-raw-content-in-range!
             ,*math-image-id-base* ,*math-image-id-limit*))))

;; Test mode
(define *math-image-test-mode* (box (not (string=? "" (getenv "NOTHELIX_TEST")))))

(define (math-image-test-mode?)
  (unbox *math-image-test-mode*))

(define (math-image-mock-result)
  *math-image-test-result*)

(define (set-math-image-test-mode! val)
  (set-box! *math-image-test-mode* val))

;; Mock SVG JSON (160x80 rect) for test mode.
(define *math-image-test-result*
  "{\"b64\":\"PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxNjAiIGhlaWdodD0iODAiIHZpZXdCb3g9IjAgMCAxNjAgODAiPjxyZWN0IHdpZHRoPSIxNjAiIGhlaWdodD0iODAiIGZpbGw9IndoaXRlIi8+PC9zdmc+\",\"width\":160,\"height\":80,\"error\":\"\"}")

;; JSON result parsing

(define (parse-math-image-result json)
  (with-handler
    (lambda (_) #false)
    (let* ([after-key (cadr (split-once json "\"b64\":\""))]
           [b64+rest  (split-once after-key "\",\"width\":")]
           [b64       (car b64+rest)]
           [w+rest    (split-once (cadr b64+rest) ",\"height\":")]
           [width     (string->number (car w+rest))]
           [h+rest    (split-once (cadr w+rest) ",\"error\":\"")]
           [height    (string->number (car h+rest))]
           [err       (car (split-once (cadr h+rest) "\"}"))])
      (and b64 width height (list b64 width height err)))))

;; Sizing

;; Wraps Rust math-image-grid; floats cross the dylib FFI as strings.
(define (math-image-size width height max-rows aspect)
  (define reply (math-image-grid width height max-rows
                                 (number->string aspect)
                                 (number->string (unbox *math-image-pt-per-row*))))
  (define parts (string-split reply ","))
  (if (= (length parts) 2)
      (cons (string->number (car parts)) (string->number (cadr parts)))
      (cons 2 10)))

(define (math-image-center-offset cols)
  (if (unbox *math-image-center*)
      (let ([area (editor-focused-buffer-area)])
        (if area
            (quotient (max 0 (- (area-width area) cols)) 2)
            0))
      0))

(define (math-image-mask-width)
  (let ([override (unbox *math-image-width-override*)])
    (if (and override (> override 0))
        override
        (let ([area (editor-focused-buffer-area)])
          (if area (max 1 (area-width area)) 240)))))

;; Display-config setters — project-config.scm applies these from .nothelix.scm.
(define (set-math-image-font-pt! n) (set-box! *math-image-font-pt* n))
(define (set-math-image-color! hex)
  ;; honour an explicit colour by turning off theme auto-derivation.
  (set-box! *math-image-text-color* hex)
  (set-box! *math-image-auto-color* #false))
(define (set-math-image-width-override! n) (set-box! *math-image-width-override* n))

(define (n-copies n x)
  (if (<= n 0) '() (cons x (n-copies (- n 1) x))))

;; Must be real spaces; an empty string paints nothing and the source bleeds through.
(define (math-image-blank-row width)
  (make-string (max 1 width) #\space))

(define (math-image-extend-row row len width)
  (if (>= len width)
      row
      (string-append row (make-string (- width len) #\space))))

;; Block detection
(define (single-line-block-body body)
  (and (string-starts-with? body "$$")
       (string-suffix? body "$$")
       (> (string-length body) 4)
       (string-trim (substring body 2 (- (string-length body) 2)))))

;; Body after a Julia "# " marker, or #false. A bare "#" is an empty body.
(define (md-comment-body line)
  (let ([t (if (string-suffix? line "\n")
               (substring line 0 (- (string-length line) 1))
               line)])
    (cond
      [(string=? t "#") ""]
      [(string-starts-with? t "# ") (substring t 2 (string-length t))]
      [else #false])))

(define (display-math-block-ranges rope total-lines)
  (define (line-body idx)
    (and (>= idx 0) (< idx total-lines)
         (md-comment-body (text.rope->string (text.rope->line rope idx)))))
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

;; (anchor close . content-lines) for the block around line-idx, or #false.
(define (find-display-math-block rope total-lines line-idx)
  (define (body idx)
    (and (>= idx 0) (< idx total-lines)
         (md-comment-body (text.rope->string (text.rope->line rope idx)))))

  (let search-up ([idx line-idx])
    (cond
      [(< idx 0) #false]
      [else
       (define b (body idx))
       (cond
         [(equal? b "$$")
          (collect-block rope total-lines idx body)]
         [(single-line-block-body b)
          => (lambda (inner)
               (cons idx (cons idx (list inner))))]
         [b (search-up (- idx 1))]
         [else #false])])))

(define (collect-block rope total-lines opener-line body)
  (define content-lines '())
  (let search-down ([idx (+ opener-line 1)])
    (cond
      [(>= idx total-lines) #false]
      [else
       (define b (body idx))
       (cond
         [(equal? b "$$")
          (cons opener-line (cons idx (reverse content-lines)))]
         [b
          (set! content-lines (cons b content-lines))
          (search-down (+ idx 1))]
         [else #false])])))

;; FFI wrappers (mockable in test mode)
(define *hex-digits* "0123456789abcdef")
(define (byte->hex2 n)
  (let ([v (max 0 (min 255 n))])
    (string-append
      (make-string 1 (string-ref *hex-digits* (quotient v 16)))
      (make-string 1 (string-ref *hex-digits* (modulo v 16))))))

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

;; Rendering a single block

;; djb2; same algorithm as image-cache.scm / table-image.scm.
(define (math-block-hash s)
  (let loop ([i 0] [h 5381])
    (if (>= i (string-length s))
        h
        (loop (+ i 1)
              (modulo (+ (* h 33) (char->integer (string-ref s i)))
                      2147483647)))))

(define (math-block-image-id doc-id anchor-line latex)
  (+ *math-image-id-base*
     (modulo (+ (math-block-hash (number->string anchor-line))
                (math-block-hash latex))
             8000000)))

;; Place a rendered SVG over exactly the block's source span (block-line-count = close-open+1).
(define (place-svg-image-at-line! result-json image-id rope anchor-line block-line-count label center? mask-width-override)
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
     (define natural-rows (car size))
     (define base-cols (cdr size))
     (define span (max 1 block-line-count))
     (define span-rows (max 1 (min natural-rows span)))
     ;; Scale cols with the clamped rows to preserve aspect ratio.
     (define span-cols
       (if (< span-rows natural-rows)
           (max 1 (quotient (+ (* base-cols span-rows) (quotient natural-rows 2))
                            natural-rows))
           base-cols))
     (define mask-width (if mask-width-override mask-width-override (math-image-mask-width)))
     (define avail-cols (max 1 (- mask-width (unbox *math-image-side-margin*))))
     (define over-wide? (> span-cols avail-cols))
     (define cols (if over-wide? avail-cols span-cols))
     (define image-rows
       (if over-wide?
           (max 1 (quotient (+ (* span-rows avail-cols) (quotient span-cols 2)) span-cols))
           span-rows))
     (define payload (kitty-placeholder-payload b64 image-id))
     (define placeholder-rows (kitty-placeholder-rows image-id cols image-rows))

     (cond
       [(string-starts-with? payload "ERROR:")
        (debug-log (string-append label ": payload error: " payload))
        #false]
       [(= (string-length placeholder-rows) 0)
        (debug-log (string-append label ": placeholder grid too large"))
        #false]
       [else
         (define char-idx (text.rope-line->char rope anchor-line))
         (define offset (if center? (math-image-center-offset cols) 0))
         (define image-row-width (+ offset cols))
         (define image-lines
           (map (lambda (row)
                  (math-image-extend-row
                    (string-append (make-string offset #\space) row)
                    image-row-width
                    mask-width))
                (string-split placeholder-rows "\n")))
         (define pad-count (max 0 (- span image-rows)))
         (define pad-lines (n-copies pad-count (math-image-blank-row mask-width)))
         (define all-rows (string-join (append image-lines pad-lines) "\n"))
         (call-add-raw-content-with-placeholders! payload span cols all-rows char-idx)
         (debug-log
           (string-append label ": registered id=" (number->string image-id)
                          " anchor=" (number->string anchor-line)
                          " span=" (number->string span)
                          " image-rows=" (number->string image-rows)
                          " cols=" (number->string cols)
                          " offset=" (number->string offset)))
         #true])]))

(define (render-display-math-block rope total-lines anchor-line close-line content-lines)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define latex (string-join content-lines "\n"))
  (define image-id (math-block-image-id doc-id anchor-line latex))
  (define block-line-count (+ 1 (- close-line anchor-line)))
  (define result-json (call-render-math-to-svg latex (unbox *math-image-font-pt*)))
  (place-svg-image-at-line! result-json image-id rope anchor-line block-line-count "math-image" #true #false))

;; Record-separator delimiter framing the batch FFI (matches Rust BATCH_SEP).
(define *math-batch-sep* (make-string 1 (integer->char 30)))

;; Bumped per render; a stale in-flight poll skips placement if it no longer matches.
(define *math-render-generation* (box 0))
(define *math-poll-interval-ms* 60)
(define *math-poll-max-attempts* 400)

(define (bump-math-render-generation!)
  (set-box! *math-render-generation* (+ 1 (unbox *math-render-generation*)))
  (unbox *math-render-generation*))

(define (collect-math-jobs rope total-lines)
  (let loop ([ranges (display-math-block-ranges rope total-lines)] [acc '()])
    (if (null? ranges)
        (reverse acc)
        (let ([block (find-display-math-block rope total-lines (car (car ranges)))])
          (loop (cdr ranges)
                (if block
                    (cons (list (car block) (cadr block) (string-join (cddr block) "\n")) acc)
                    acc))))))

(define (job-anchor job) (car job))
(define (job-close job) (cadr job))
(define (job-latex job) (caddr job))

(define (place-math-job rope doc-id job result-json)
  (define anchor (job-anchor job))
  (define image-id (math-block-image-id doc-id anchor (job-latex job)))
  (define block-line-count (+ 1 (- (job-close job) anchor)))
  (place-svg-image-at-line! result-json image-id rope anchor block-line-count "math-image" #true #false))

(define (block-count-phrase n)
  (string-append (number->string n) " block" (if (= n 1) "" "s")))

(define (result->natural-rows result-json)
  (define result (parse-math-image-result result-json))
  (if (and result (= (string-length (list-ref result 3)) 0))
      (car (math-image-size (list-ref result 1) (list-ref result 2)
                            (unbox *math-image-target-rows*)
                            (unbox *math-image-cell-aspect*)))
      0))

(define (build-reservation-spec results)
  (string-join (map (lambda (r) (number->string (result->natural-rows r))) results) ","))

(define (reserve-buffer-math-lines! doc-id reserved)
  (define rope (editor->text doc-id))
  (define doc-len (text.rope-len-chars rope))
  (define r (helix.static.range 0 doc-len))
  (define sel (helix.static.range->selection r))
  (helix.static.set-current-selection-object! sel)
  (helix.static.replace-selection-with reserved)
  (helix.static.collapse_selection)
  (helix.static.commit-changes-to-history)
  (schedule-reconceal 50))

(define (render-display-math-async blob current-text doc-id)
  (define color (effective-math-text-color))
  (define font-pt (unbox *math-image-font-pt*))
  (define block-count (length (string-split blob *math-batch-sep*)))
  (define my-gen (bump-math-render-generation!))
  (set-status! (string-append "math-image: rendering " (block-count-phrase block-count) "…"))
  (define job-id (start-render-batch blob font-pt color))
  (poll-math-batch! job-id current-text doc-id block-count my-gen 0))

(define (place-results-over-blocks rope doc-id results)
  (define total (text.rope-len-lines rope))
  (define jobs (collect-math-jobs rope total))
  (try-clear-math-image-band!)
  (let place ([js jobs] [rs results] [placed 0])
    (if (or (null? js) (null? rs))
        (set-status! (string-append "math-image: rendered " (block-count-phrase placed)))
        (place (cdr js) (cdr rs)
               (if (place-math-job rope doc-id (car js) (car rs))
                   (+ placed 1)
                   placed)))))

(define (poll-math-batch! job-id current-text doc-id block-count my-gen attempts)
  (when (= my-gen (unbox *math-render-generation*))
    (define reply (poll-render-batch job-id))
    (cond
      [(string=? reply "PENDING")
       (if (< attempts *math-poll-max-attempts*)
           (enqueue-thread-local-callback-with-delay *math-poll-interval-ms*
             (lambda () (poll-math-batch! job-id current-text doc-id block-count my-gen (+ attempts 1))))
           (set-status! "math-image: render timed out"))]
      [(string-starts-with? reply "ERROR:")
       (set-status! (string-append "math-image: " reply))]
      [else
       (define results (string-split reply *math-batch-sep*))
       (define live-text (text.rope->string (editor->text doc-id)))
       (cond
         [(not (equal? live-text current-text))
          (set-status! "math-image: buffer changed during render — re-run")]
         [(not (= (length results) block-count))
          (set-status! "math-image: render count mismatch")]
         [else
          (define reserved (reserve-math-lines current-text (build-reservation-spec results)))
          (when (not (equal? reserved current-text))
            (reserve-buffer-math-lines! doc-id reserved))
          (place-results-over-blocks (editor->text doc-id) doc-id results)])])))

;; Public commands

;;@doc
;; Render the display math block under the cursor as a Typst SVG image.
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
     (define close-line (cadr block))
     (define content-lines (cddr block))
     (if (render-display-math-block rope total-lines anchor-line close-line content-lines)
         (set-status! "math-image: rendered display math")
         (set-status! "math-image: render failed (see log)"))]))

(define (render-display-math-test-mode rope doc-id)
  (define jobs (collect-math-jobs rope (text.rope-len-lines rope)))
  (bump-math-render-generation!)
  (try-clear-math-image-band!)
  (cond
    [(null? jobs)
     (set-status! "math-image: no display math to render")]
    [else
     (for-each
       (lambda (job)
         (place-math-job rope doc-id job
                         (call-render-math-to-svg (job-latex job) (unbox *math-image-font-pt*))))
       jobs)
     (set-status! (string-append "math-image: rendered " (block-count-phrase (length jobs))))]))

;;@doc
;; Render every `# $$ ... # $$` display math block in the buffer as an image.
(define (render-all-display-math)
  (define focus (editor-focus))
  (define doc-id (editor->doc-id focus))
  (define path (editor-document->path doc-id))
  (define rope (editor->text doc-id))
  (cond
    [(not (file-has-conceal-extension? path))
     (set-status! "math-image: not a math-enabled file type")]
    [(math-image-test-mode?)
     (render-display-math-test-mode rope doc-id)]
    [else
     (define current-text (text.rope->string rope))
     (define blob (math-block-latex-batch current-text))
     (cond
       [(= (string-length blob) 0)
        (bump-math-render-generation!)
        (try-clear-math-image-band!)
        (set-status! "math-image: no display math to render")]
       [else
        (render-display-math-async blob current-text doc-id)])]))

;;@doc
;; Remove math-image RawContent from the view, leaving plot and table images intact.
(define (clear-math-images)
  (bump-math-render-generation!)
  (try-clear-math-image-band!)
  (set-status! "math-image: cleared"))
