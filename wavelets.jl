# Nothelix Notebook - Runnable Julia Script
# Source: /Users/alielali/projects/helix/wavelength_analysis.ipynb
#
# This file can be:
# - Run directly: julia this_file.jl
# - Edited and executed cell-by-cell in Helix with Nothelix
# - Synced back to .ipynb with :sync-to-ipynb

# No-op macro for cell markers (allows standalone execution)
macro cell(idx, exec_count) end
macro markdown(idx) end

# ═══════════════════════════════════════════════════════════════════
@markdown 0
#=
# 

$$
\newcommand{\ip}[2]{\langle#1,#2\rangle}
\newcommand{\norm}[1]{\|#1\|}
\newcommand{\abs}[1]{\left|#1\right|}
\newcommand{\T}{\text{${}^{\text{T}}$}}
\newcommand{\R}{\mathbb{R}}
\newcommand{\argmin}[1]{\underset{#1}{\operatorname{argmin}}}
$$
=#

# ═══════════════════════════════════════════════════════════════════
@cell 1 62
import Pkg; Pkg.add("Wavelets"); Pkg.add("InteractiveViz")
using Wavelets, LinearAlgebra, Plots, ContinuousWavelets, Random


# ─── Output ───
# stderr:    Resolving package versions...
  No Changes to `~/.julia/environments/v1.11/Project.toml`
  No Changes to `~/.julia/environments/v1.11/Manifest.toml`
   Resolving package versions...
  No Changes to `~/.julia/environments/v1.11/Project.toml`
  No Changes to `~/.julia/environments/v1.11/Manifest.toml`

# ─────────────

# ═══════════════════════════════════════════════════════════════════
@markdown 2
#=
This tutorial mostly uses the Wavelets.jl toolbox to analyse signals and
gain understanding. Refer to [Wavelets.jl
docs](https://github.com/JuliaDSP/Wavelets.jl) and the
[ContinuousWavelets.jl
docs](https://github.com/UCD4IDS/ContinuousWavelets.jl) for function
syntax. 

# Q1. Denoising

Consider the signal with added noise
=#

# ═══════════════════════════════════════════════════════════════════
@cell 3 78
t = 0:0.001:1
t = t/1023
y = sin.(2π*10*t) + 0.5*randn(length(t))



# ═══════════════════════════════════════════════════════════════════
@markdown 4
#=
(a)  Use `denoise` from Wavelets.jl with different thresholds (hard/soft)
    for Haar.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 5 79
# Pad to nearest power of 2
function pad_to_power_of_2(signal)
    n = length(signal)
    next_pow2 = 2^ceil(Int, log2(n))
    padded = zeros(eltype(signal), next_pow2)
    padded[1:n] = signal
    return padded
end

# Create original signal with noise
y_original = sin.(2π*10*t)
y_noisy = y_original + 0.5*randn(length(t))

# Pad the signal
y_padded = pad_to_power_of_2(y_noisy)

# Define Haar wavelet
wave_haar = wavelet(WT.haar)
wave_haar = WT.scale(wave_haar, 1/sqrt(2))

# Denoise with TI (Translation Invariant) option
denoised_padded = denoise(y_padded, wave_haar, TI=true)

# Extract original part
denoised = denoised_padded[1:length(y_noisy)]


# ─── Output ───
# ERROR: LoadError: UndefVarError: `wavelet` not defined in `Main`
in expression starting at string:35
# ─────────────

# ═══════════════════════════════════════════════════════════════════
@markdown 6
#=
(b)  Visual the denoised signals vs the original and the noisy signal.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 7 75
using Plots
plot(t, y_noisy, label="Noisy Signal")
plot!(t, denoised, label="Denoised Signal")
xlabel!("Time")
ylabel!("Amplitude")
title!("Wavelet Denoising with Haar Wavelet")


# ─── Output ───
# ERROR: SystemError: opening file "/tmp/helix-kernel-1/input.json": No such file or directory
# ─────────────

# ═══════════════════════════════════════════════════════════════════
@markdown 8
#=
(c)  Compute the SNR improvement
=#

# ═══════════════════════════════════════════════════════════════════
@cell 9 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 10
#=
(d) Repeat steps (a)-(c) for the DB4 wavelet
=#

# ═══════════════════════════════════════════════════════════════════
@cell 11 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 12
#=
(e)  What difference do you notice between the two wavelets in terms of
    denoising quality?
=#

# ═══════════════════════════════════════════════════════════════════
@cell 13 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 14
#=
# Q2. Time-Frequency Analysis
=#

# ═══════════════════════════════════════════════════════════════════
@markdown 15
#=
Here is an example of using the continuous wavelet transform to analyse a signal.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 16 3
# Morlet wavelet configuration
c = wavelet(Morlet(π), β=2, averagingType=NoAve())
res = cwt(doppler_signal, c)

# scalogram visualization
heatmap(abs.(res)', c=:viridis, 
        title="CWT Scalogram", 
        xlabel="time", 
        ylabel="scalee")

# ═══════════════════════════════════════════════════════════════════
@markdown 17
#=
Consider the chip signal:
=#

# ═══════════════════════════════════════════════════════════════════
@cell 18 4
t = 0:0.001:1
y = sin.(2π*100*t.^2);

# ═══════════════════════════════════════════════════════════════════
@markdown 19
#=
1.  Plot it
=#

# ═══════════════════════════════════════════════════════════════════
@cell 20 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 21
#=
(a)  Use a continuous wavelet transform (CWT) with the Morlet wavelet on
    $y$.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 22 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 23
#=
(b) Generate a scalogram using `heatmap`.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 24 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 25
#=
(c) Where on the scalogram is the time-frequency localisation of the
    dominant components?
=#

# ═══════════════════════════════════════════════════════════════════
@cell 26 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 27
#=
# Q3. Wavelet vs. Fourier

Consider the following signals:
=#

# ═══════════════════════════════════════════════════════════════════
@cell 28 5
bumps_signal = testfunction(length(t), "Bumps")

doppler_signal = testfunction(length(t), "Doppler")

plot(t, [bumps_signal doppler_signal],
     label=["Bumps" "Doppler"],
     xlabel="Time (s)",
     ylabel="Amplitude",
     linewidth=2)

# ═══════════════════════════════════════════════════════════════════
@markdown 29
#=
(a)  Plot the Fourier spectrum and D4 wavelet coefficients
=#

# ═══════════════════════════════════════════════════════════════════
@cell 30 nothing


# ═══════════════════════════════════════════════════════════════════
@markdown 31
#=
(b)  Explain which method better localises the transient events and why.
=#

# ═══════════════════════════════════════════════════════════════════
@cell 32 nothing



