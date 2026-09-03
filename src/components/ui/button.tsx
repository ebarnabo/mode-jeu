import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap font-semibold transition-all duration-200 active:scale-[.97] disabled:pointer-events-none disabled:opacity-40",
  {
    variants: {
      variant: {
        primary: "bg-brass text-ink hover:bg-brass/90 shadow-[0_8px_24px_-8px_rgba(214,166,74,.55)]",
        ghost: "bg-transparent text-muted hover:bg-raised hover:text-paper",
        outline: "border border-line bg-surface text-paper hover:border-muted/50",
      },
      size: {
        sm: "h-8 px-3 text-[13px] rounded-xl",
        md: "h-11 px-6 text-sm rounded-2xl",
        lg: "h-14 px-8 text-[15px] rounded-3xl",
      },
    },
    defaultVariants: { variant: "primary", size: "md" },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, ...props }, ref) => (
    <button ref={ref} className={cn(buttonVariants({ variant, size }), className)} {...props} />
  )
);
Button.displayName = "Button";
