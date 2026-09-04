/*
*******************************************************************************
* Copyright (c) 2021 by M5Stack
*                  Equipped with M5StickC-Plus sample source code
*                          配套  M5StickC-Plus 示例源代码
* Visit for more information: https://docs.m5stack.com/en/core/m5stickc_plus
* 获取更多资料请访问: https://docs.m5stack.com/zh_CN/core/m5stickc_plus
*
* Describe: QRcode.  创建二维码
* Date: 2021/9/18
*******************************************************************************
*/
#include <M5StickCPlus2.h>

int state = 0;
int prev = 0;
int STATE_MAX = 2;

void setup() {
  M5.begin();
  M5.Lcd.setTextSize(3);

  M5.Lcd.qrcode("http://portfolio.pwll.dev", 0, 0,
                135);
  M5.Lcd.print("\n\n\n\n\n\nKOMOTO\n  Kenta");
}

void loop() {
  M5.update();

  if (M5.BtnA.wasPressed()) {
    state = (state + 1) % STATE_MAX;
  }

  if (state != prev) {
    if (state == 0) {
      M5.Display.clear();
      M5.Lcd.qrcode("http://portfolio.pwll.dev", 0, 0,
                    135);
      M5.Speaker.tone(8000, 20);
      M5.Lcd.setCursor(0, 0);
      M5.Lcd.print("\n\n\n\n\n\nKOMOTO\n Kenta");
    } else {
      M5.Display.clear();
      M5.Lcd.qrcode("http://sudoku.pwll.dev", 0, 0,
                    135);
      M5.Speaker.tone(8000, 20);
      M5.Lcd.setCursor(0, 0);
      M5.Lcd.print("\n\n\n\n\n\nSudoku\n Solver");
    }

    prev = state;
  }
}