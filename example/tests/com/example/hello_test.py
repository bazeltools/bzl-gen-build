import unittest

from com.example.hello import some_data

class HelloTest(unittest.TestCase):
    def test_empty(self):
        self.assertEqual(some_data().size, 0)

if __name__ == '__main__':
    unittest.main()
