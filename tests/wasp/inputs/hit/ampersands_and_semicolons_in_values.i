

value_wt_quotes_ampersand_beg = '&if((x<1)(y>0.5),1E7,-1E7)'
value_wt_quotes_ampersand_mid = 'if((x<1)&(y>0.5),1E7,-1E7)'
value_wt_quotes_ampersand_end = 'if((x<1)(y>0.5),1E7,-1E7)&'

value_no_quotes_ampersand_beg = &if((x<1)(y>0.5),1E7,-1E7)
value_no_quotes_ampersand_mid = if((x<1)&(y>0.5),1E7,-1E7)
value_no_quotes_ampersand_end = if((x<1)(y>0.5),1E7,-1E7)&

value_no_quotes_semicolon_beg = ;if((x<1)(y>0.5),1E7,-1E7)
value_no_quotes_semicolon_mid = if((x<1);(y>0.5),1E7,-1E7)
value_no_quotes_semicolon_end = if((x<1)(y>0.5),1E7,-1E7);

