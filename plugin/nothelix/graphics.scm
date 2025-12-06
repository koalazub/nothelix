;;; graphics.scm - Graphics protocol detection and image rendering
;;;
;;; Uses viuer for protocol detection and our custom implementation
;;; for generating escape sequences (which don't print directly to stdout).

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")  ; For set-status!

;; FFI imports for graphics functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          config-get-protocol
                          detect-graphics-protocol
                          render-image-bytes
                          render-image-b64-bytes
                          viuer-protocol
                          image-detect-format
                          image-detect-format-bytes
                          notebook-cell-image-data))

(provide graphics-protocol
         graphics-check
         nothelix-status
         render-image
         render-image-b64
         render-cell-image
         get-image-escape-seq
         get-image-b64-escape-seq
         test-kitty-image)

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Protocol Detection (via viuer)
;;; ─────────────────────────────────────────────────────────────────────────────

;; Get the graphics protocol viuer will use
;; Returns: "kitty", "iterm", or "block"
(define (graphics-protocol)
  (viuer-protocol))

;; Check graphics capability and report to user
;; Returns: #t if graphics available, #f otherwise
(define (graphics-check)
  (define protocol (graphics-protocol))
  (define msg
    (cond
      [(equal? protocol "kitty")
       "Graphics: Kitty protocol (full colour, efficient)"]
      [(equal? protocol "iterm")
       "Graphics: iTerm2 protocol (inline images)"]
      [else
       "Graphics: Unicode halfblocks (fallback)"]))
  (set-status! msg)
  (not (equal? protocol "block")))

;; Show full nothelix status
(define (nothelix-status)
  (define protocol (graphics-protocol))
  (set-status!
    (string-append "Nothelix | Graphics: " protocol " (via viuer)")))

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Image Rendering (returns escape sequences, does NOT print to stdout)
;;; ─────────────────────────────────────────────────────────────────────────────

;; Get escape sequence bytes for an image file.
;; Returns the raw escape sequence string that can be written to terminal.
;; Does NOT print directly - caller must handle output.
;;
;; Args:
;;   path: Path to image file
;;   width: Width in terminal columns (0 = auto)
;;   height: Height in terminal rows (0 = auto)
;;
;; Returns: escape sequence string on success, #f on error
(define (get-image-escape-seq path width height)
  (define result (render-image-bytes path width height))
  (cond
    [(string-starts-with? result "ERROR:")
     (set-status! result)
     #f]
    [else result]))

;; Get escape sequence bytes for base64-encoded image.
;; Returns the raw escape sequence string.
;;
;; Args:
;;   b64-data: Base64-encoded image data
;;   width: Width in terminal columns (0 = auto)
;;   height: Height in terminal rows (0 = auto)
;;
;; Returns: escape sequence string on success, #f on error
(define (get-image-b64-escape-seq b64-data width height)
  (define result (render-image-b64-bytes b64-data width height))
  (cond
    [(string-starts-with? result "ERROR:")
     (set-status! result)
     #f]
    [else result]))

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Legacy API (for compatibility with existing code)
;;; ─────────────────────────────────────────────────────────────────────────────

;; Render an image file (legacy API)
;; Returns escape sequence bytes for caller to handle
(define (render-image path rows char-idx)
  (get-image-escape-seq path 0 rows))

;; Render base64 image data (legacy API)
;; Returns escape sequence bytes for caller to handle
(define (render-image-b64 b64-data rows char-idx)
  (get-image-b64-escape-seq b64-data 0 rows))

;; Render image output from a notebook cell
;; Returns escape sequence bytes for caller to handle
(define (render-cell-image notebook-path cell-index char-idx rows)
  (define b64-data (notebook-cell-image-data notebook-path cell-index))
  (cond
    [(equal? b64-data "") #f]  ;; No image in cell
    [(string-starts-with? b64-data "ERROR:")
     (set-status! b64-data)
     #f]
    [else
     (render-image-b64 b64-data rows char-idx)]))

;;; ─────────────────────────────────────────────────────────────────────────────
;;; Debug/Test Functions
;;; ─────────────────────────────────────────────────────────────────────────────

;; FFI import for testing
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          kitty-display-image
                          write-raw-to-tty))

;; Test Kitty graphics by displaying a tiny red square
;; Usage: :scm (test-kitty-image)
(define (test-kitty-image)
  ;; Tiny 2x2 red PNG (base64 encoded)
  ;; This is a valid PNG that should display as a small red square
  (define tiny-red-png "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAADklEQVQI12P4z8DAwMAAAw4B/xnq5OkAAAAASUVORK5CYII=")
  
  (define protocol (graphics-protocol))
  (set-status! (string-append "Testing Kitty graphics (protocol: " protocol ")"))
  
  ;; Get the escape sequence
  (define escape-seq-b64 (kitty-display-image tiny-red-png 999 2))
  
  (set-status! (string-append "Escape seq length: " (number->string (string-length escape-seq-b64)) " bytes (b64)"))
  
  ;; Write to TTY
  (define result (write-raw-to-tty escape-seq-b64))
  
  (if (> (string-length result) 0)
      (set-status! (string-append "TTY write error: " result))
      (set-status! "TTY write succeeded - check if image appears")))
