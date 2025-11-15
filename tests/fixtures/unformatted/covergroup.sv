module cover_demo;
covergroup cg @(posedge clk);
coverpoint data {
bins low = {0,1};
bins high = {2,3};
}
endgroup
endmodule

