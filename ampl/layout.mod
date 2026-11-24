# The best integer split of T cells among n fill children with weights wt: the widths sum to T and the worst
# distance of an inner edge from its exact (fractional) position is as small as possible.
# layout.rs rounds every edge to the nearest cell, so its worst edge error is at most 1/2; this model gives the optimum
# to hold it against (ampl/run).
param n integer > 0;
param T >= 0;
param wt {1..n} > 0;
param ex {i in 0..n} = T * (sum {j in 1..i} wt[j]) / (sum {j in 1..n} wt[j]);
var w {1..n} integer >= 0;
var E >= 0;
s.t. total: sum {i in 1..n} w[i] = T;
s.t. up {i in 1..n-1}: (sum {j in 1..i} w[j]) - ex[i] <= E;
s.t. dn {i in 1..n-1}: ex[i] - (sum {j in 1..i} w[j]) <= E;
minimize worst: E;
