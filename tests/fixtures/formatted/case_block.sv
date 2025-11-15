module case_demo;
  always_comb begin
    case (sel)
      2 'b 0    : foo = 0;
      4 'b 1010 : foo = 1;
      default : foo = 2;
    endcase
  end
endmodule
