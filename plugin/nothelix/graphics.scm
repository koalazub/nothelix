;;; graphics.scm - Graphics protocol detection and image rendering

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For set-status!

;; FFI imports for graphics functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          config-get-protocol
                          detect-graphics-protocol
                          render-image-for-protocol
                          render-b64-for-protocol
                          image-detect-format
                          image-detect-format-bytes
                          notebook-cell-image-data))

(provide graphics-protocol
         graphics-check
         nothelix-status
         render-image
         render-image-b64
         render-cell-image
         next-image-id
         *raw-content-available*)

;; Current active protocol (cached after first detection)
(define *active-protocol* #f)
(define *protocol-checked* #f)

;; Image ID counter for unique IDs
(define *image-id-counter* 1)

(define (next-image-id)
  (define id *image-id-counter*)
  (set! *image-id-counter* (+ *image-id-counter* 1))
  id)

;; Check if add-raw-content! API is available in this helix build
;; feature/inline-image-rendering branch has this binding registered globally
;; in helix-term/src/commands/engine/steel/mod.rs:5762
;; It's a global function, NOT imported from libnothelix
(define *raw-content-available* #t)

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Protocol Detection and Configuration
;;; ─────────────────────────────────────────────────────────────────────────────

;; Get the active graphics protocol
;; Checks config first (for user override), then auto-detects
;; Returns: "kitty", "iterm2", "sixel", or "none"
(define (graphics-protocol)
  (when (not *protocol-checked*)
    (define config-protocol (config-get-protocol))
    (set! *active-protocol*
          (if (equal? config-protocol "auto")
              (detect-graphics-protocol)
              config-protocol))
    (set! *protocol-checked* #t))
  *active-protocol*)

;; Check graphics capability and report to user
;; Returns: #t if graphics available, #f otherwise
(define (graphics-check)
  (define protocol (graphics-protocol))
  (define msg
    (cond
      [(equal? protocol "kitty")
       "Graphics: Kitty protocol (full colour, efficient caching)"]
      [(equal? protocol "iterm2")
       "Graphics: iTerm2 protocol (inline images)"]
      [(equal? protocol "sixel")
       "Graphics: Sixel protocol (limited colour support)"]
      [else
       "Graphics: None (text placeholders only)"]))
  (set-status! msg)
  (not (equal? protocol "none")))

;; Show full nothelix status
(define (nothelix-status)
  (define protocol (graphics-protocol))
  (define config-protocol (config-get-protocol))
  (define raw-api (if *raw-content-available* "available" "not available"))
  (set-status!
    (string-append
      "Nothelix | Protocol: " protocol
      " (config: " config-protocol ")"
      " | RawContent API: " raw-api)))

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Image Rendering (Rust-backed, format-aware)
;;; ─────────────────────────────────────────────────────────────────────────────

;; All escape sequence generation is handled in Rust.
;; Steel is purely orchestration - no terminal escape codes here.

;; Render an image file for the current protocol
;; Returns: escape sequence string, or error message
(define (render-image path rows char-idx)
  (define protocol (graphics-protocol))
  (define id (next-image-id))

  (cond
    [(equal? protocol "none")
     (set-status! "No graphics protocol available")
     #f]
    [else
     ;; Rust handles: format detection, conversion, escape sequence generation
     (define escape-seq (render-image-for-protocol path protocol id))
     (cond
       [(string-starts-with? escape-seq "ERROR:")
        (set-status! escape-seq)
        #f]
       [(not *raw-content-available*)
        ;; RawContent API not yet available - show info
        (define format (image-detect-format path))
        (set-status!
          (string-append "[" protocol "/" format " image ready] (RawContent API pending)"))
        #f]
       [else
        ;; Full rendering path (when API available)
        ;; (add-raw-content! escape-seq rows char-idx)
        rows])]))

;; Render base64 image data for the current protocol
(define (render-image-b64 b64-data rows char-idx)
  (define protocol (graphics-protocol))
  (define id (next-image-id))

  (cond
    [(equal? protocol "none")
     (set-status! "No graphics protocol available")
     #f]
    [else
     ;; Rust handles everything
     (define escape-seq (render-b64-for-protocol b64-data protocol id))
     (cond
       [(string-starts-with? escape-seq "ERROR:")
        (set-status! escape-seq)
        #f]
       [(not *raw-content-available*)
        (define format (image-detect-format-bytes b64-data))
        (set-status!
          (string-append "[" protocol "/" format " image: "
                         (number->string (quotient (string-length b64-data) 1024))
                         "KB] (RawContent API pending)"))
        #f]
       [else
        ;; Convert string escape sequence to bytes and add to document
        (define payload (string->bytes escape-seq))
        (add-raw-content! payload rows char-idx)
        rows])]))

;; Render image output from a notebook cell
(define (render-cell-image notebook-path cell-index char-idx rows)
  (define b64-data (notebook-cell-image-data notebook-path cell-index))
  (cond
    [(equal? b64-data "") #f]  ;; No image in cell
    [(string-starts-with? b64-data "ERROR:")
     (set-status! b64-data)
     #f]
    [else
     (render-image-b64 b64-data rows char-idx)]))
