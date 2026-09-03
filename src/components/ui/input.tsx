import * as React from "react";
import { cn } from "@/lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "h-11 w-full rounded-2xl border border-line bg-raised px-4 text-sm text-paper placeholder:text-muted/70",
        "transition-colors duration-200 focus:border-brass/60",
        className
      )}
      {...props}
    />
  )
);
Input.displayName = "Input";
