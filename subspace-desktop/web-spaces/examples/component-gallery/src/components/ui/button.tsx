// Locally owned shadcn/ui Button composition. See public/licences/shadcn-MIT.txt.
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { Children, type ComponentProps } from 'react';
import { cn } from '@/lib/utils';

const buttonVariants = cva(
  'inline-flex shrink-0 items-center justify-center whitespace-nowrap border font-normal outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--ring)] disabled:pointer-events-none [&_svg]:shrink-0',
  {
    variants: {
      variant: {
        default: 'button-primary',
        destructive: 'button-reject',
        success: 'button-accept',
        warning: 'button-caution',
        outline: 'button-outline',
        ghost: 'button-ghost',
        selected: 'button-selected',
      },
      size: { default: 'action-button', icon: 'copy-button' },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
);

type ButtonProps = ComponentProps<'button'> & VariantProps<typeof buttonVariants> & {
  asChild?: boolean;
};

function Button({ className, variant, size, asChild = false, type = 'button', children, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : 'button';
  return <Comp data-slot="button" type={type} className={cn(buttonVariants({ variant, size, className }))} {...props}>
    {Children.map(children, child => typeof child === 'string' || typeof child === 'number'
      ? <span className="button-label">{child}</span>
      : child)}
  </Comp>;
}

export { Button, buttonVariants };
