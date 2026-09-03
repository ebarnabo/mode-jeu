import * as React from "react";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import { cn } from "@/lib/utils";

export const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitive.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitive.Root
    ref={ref}
    className={cn(
      "peer inline-flex h-[26px] w-[46px] shrink-0 cursor-pointer items-center rounded-full border border-transparent transition-colors duration-200",
      "data-[state=checked]:bg-jade data-[state=unchecked]:bg-line disabled:cursor-not-allowed disabled:opacity-40",
      className
    )}
    {...props}
  >
    <SwitchPrimitive.Thumb className="pointer-events-none block h-[20px] w-[20px] rounded-full bg-white shadow-md transition-transform duration-200 ease-[cubic-bezier(.34,1.56,.64,1)] data-[state=checked]:translate-x-[23px] data-[state=unchecked]:translate-x-[3px]" />
  </SwitchPrimitive.Root>
));
Switch.displayName = "Switch";
