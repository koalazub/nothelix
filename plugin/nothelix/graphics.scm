;;; graphics.scm — Graphics protocol detection and image rendering

(require "string-utils.scm")
(require "helix/editor.scm")
(require "helix/misc.scm")

(#%require-dylib "libnothelix"
                 (only-in nothelix
                          render-image-b64-bytes
                          viuer-protocol))

(provide graphics-protocol
         graphics-check
         nothelix-status
         get-image-b64-escape-seq)

;; Protocol detection

;;@doc
;; Return the active graphics protocol as a string: "kitty", "iterm", or "block".
(define (graphics-protocol)
  (viuer-protocol))

;;@doc
;; Report the active graphics protocol to the status bar.
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

;; Image rendering (returns escape sequences, does not print to stdout)

;;@doc
;; Return the terminal escape sequence for base64-encoded image data, or #false on error.
(define (get-image-b64-escape-seq b64-data width height)
  (define result (render-image-b64-bytes b64-data width height))
  (if (string-starts-with? result "ERROR:")
      (begin (set-status! result) #false)
      result))
