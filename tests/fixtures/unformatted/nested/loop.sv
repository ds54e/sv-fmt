module loop_demo;
always_ff @(posedge clk) begin
for(i=0;i<2;i++)
  data[i] <= 0;
end
endmodule

