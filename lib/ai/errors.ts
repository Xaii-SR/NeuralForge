export class AIError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = "AIError";
    this.code = code;
  }
}