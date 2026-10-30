
[TopLevelBlock]

  [./sub-decl-sub-term-no-type]
    flag01  = 1.1
    array01 = 'name11 name12 name13'
  [../]

  [./sub-decl-top-term-no-type]
    flag02  = 2.2
    array02 = 'name21 name22 name23'
  []

  [top-decl-sub-term-no-type]
    flag03  = 3.3
    array03 = 'name31 name32 name33'
  [../]

  [top-decl-top-term-no-type]
    flag04  = 4.4
    array04 = 'name41 name42 name43'
  []

  [./sub-decl-sub-term-wt-type]
    type    = SubDeclSubTerm
    flag05  = 5.5
    array05 = 'name51 name42 name53'
  [../]

  [./sub-decl-top-term-wt-type]
    type    = SubDeclTopTerm
    flag06  = 6.6
    array06 = 'name61 name62 name63'
  []

  [top-decl-sub-term-wt-type]
    type    = TopDeclSubTerm
    flag07  = 7.7
    array07 = 'name71 name72 name73'
  [../]

  [top-decl-top-term-wt-type]
    type    = TopDeclTopTerm
    flag08  = 8.8
    array08 = 'name81 name82 name83'
  []

[]
