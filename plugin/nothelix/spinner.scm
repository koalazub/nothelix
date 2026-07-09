;;; spinner.scm — Loading spinner animation for cell execution

(provide spinner-next-frame
         spinner-reset)

;; Rotating double-helix frames (Braille patterns).
(define spinner-frames
  (vector "⠿⠶⠿⠶⠿"
          "⠾⠷⠾⠷⠾"
          "⠼⠧⠼⠧⠼"
          "⠸⠏⠸⠏⠸"
          "⠴⠋⠴⠋⠴"
          "⠦⠙⠦⠙⠦"
          "⠧⠹⠧⠹⠧"
          "⠇⠸⠇⠸⠇"
          "⠏⠼⠏⠼⠏"
          "⠋⠴⠋⠴⠋"
          "⠙⠦⠙⠦⠙"
          "⠹⠧⠹⠧⠹"))

(define *spinner-frame* 0)

;;@doc
;; Return the next spinner frame string and advance the counter.
(define (spinner-next-frame)
  (define frame (vector-ref spinner-frames *spinner-frame*))
  (set! *spinner-frame* (modulo (+ *spinner-frame* 1) (vector-length spinner-frames)))
  frame)

;;@doc
;; Reset the spinner to the first frame.
(define (spinner-reset)
  (set! *spinner-frame* 0))
