
[Simple]
  foo01=${fparse 42}
  foo02='${fparse 42}'
[]

[Multiple]
  foo01=${fparse 42} bar01=${fparse foo01}
  foo02='${fparse 42}' bar02='${fparse foo02}'
[]

[Fparse]
  pp_ini_bc = ${fparse if(depth<=2000, 1000*9.8*depth, 1000*9.8*depth+(depth-2000)*9.8*1000)}
[]

[Examples]
  sliceheight = ${ ${fuelheight} ${numslices}}
  file_base01 = ${raw ${postprocessor_type_02} _fv}
  file_base02 = '${raw ${postprocessor_type_01} _fv}'
[]

[NoQuotedNestings]
  slice_01 = ${/ ${fuel ${numslices} height} }
  slice_02 = ${${fuel ${numslices} height}}
  slice_03 = ${${${numslices} height}}
  slice_04 = ${${${numslices}}}
  slice_05 = ${raw ${postprocessor ${numslices} _01} _fv}
[]

[SingleQuotedArrays]
  ndice_01 = '${/ ${fuel ${numslices} height} }'
  ndice_02 = '${${fuel ${numslices} height}}'
  ndice_03 = '${${${numslices} height}}'
  ndice_04 = '${${${numslices}}}'
  ndice_05 = '${raw ${postprocessor ${numslices} _01} _fv}'
[]

[DoubleQuotedStrings]
  nchop_01 = "${/ ${fuel ${numslices} height} }"
  nchop_02 = "${${fuel ${numslices} height}}"
  nchop_03 = "${${${numslices} height}}"
  nchop_04 = "${${${numslices}}}"
  nchop_05 = "${raw ${postprocessor ${numslices} _01} _fv}"
[]
