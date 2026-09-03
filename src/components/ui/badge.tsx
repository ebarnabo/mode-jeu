import * as React from "react";
import { cn } from "@/lib/utils";

export const Badge = ({ className, ...props }: React.HTMLAttributes<HTMLSpanElement>) => (
  <span
    className={cn(
      "inline-flex h-6 items-center justify-center gap-1 rounded-xl border border-line bg-raised px-2.5 text-[11px] font-medium text-muted",
      className
    )}
    {...props}
  />
);
