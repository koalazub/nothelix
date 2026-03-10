;;; spinner.scm - Loading spinner animations for cell execution
;;;
;;; Displays a rotating Braille-pattern helix during async cell execution.
;;; Each frame is a different rotation angle of the double-helix shape.

(provide spinner-next-frame
         spinner-reset)

;; 12 frames of a rotating double helix using Braille patterns.
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

;; Current frame index.
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
