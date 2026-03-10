;;; graphics.scm - Graphics protocol detection and image rendering
;;;
;;; Detects the terminal's graphics protocol (Kitty / iTerm2 / block fallback)
;;; and provides functions that return escape sequence strings for inline images.
;;; These functions do NOT write to stdout directly — the caller (execution.scm)
;;; passes the sequences to Helix's RawContent API for proper in-buffer rendering.

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")

;; FFI imports for graphics functions
(#%require-dylib "libnothelix"
                 (only-in nothelix
                          render-image-b64-bytes
                          viuer-protocol))

(provide graphics-protocol
         graphics-check
         nothelix-status
         get-image-b64-escape-seq)

;;; ---------------------------------------------------------------------------
;;; Protocol Detection
;;; ---------------------------------------------------------------------------

;;@doc
;; Return the active graphics protocol as a string: "kitty", "iterm", or "block".
(define (graphics-protocol)
  (viuer-protocol))

;;@doc
;; Report the active graphics protocol to the status bar.
;; Returns #true if a real graphics protocol is available.
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

;;@doc
;; Show full nothelix status in the status bar.
(define (nothelix-status)
  (define protocol (graphics-protocol))
  (set-status!
    (string-append "Nothelix | Graphics: " protocol)))

;;; ---------------------------------------------------------------------------
;;; Image Rendering (returns escape sequences, does NOT print to stdout)
;;; ---------------------------------------------------------------------------

;;@doc
;; Return the terminal escape sequence for base64-encoded image data.
;; `width` and `height` are in terminal columns/rows (0 = auto).
;; Returns #false on error.
;; (-> string? integer? integer? (or/c string? #false))
(define (get-image-b64-escape-seq b64-data width height)
  (define result (render-image-b64-bytes b64-data width height))
  (if (string-starts-with? result "ERROR:")
      (begin (set-status! result) #false)
      result))
